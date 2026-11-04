//! The streaming writer: definitions are kept in memory, chunk data goes to
//! the file as it arrives, and every metadata structure is laid out and
//! written by [`Nc4Writer::finish`].

use std::{
    fs::File,
    io::{BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    filters,
    hdf5::{self, ChunkEntry, Filter, Message, UNDEFINED_ADDRESS},
    AttrValue, DataType, Element, Error, FillValue, Result, VariableOptions,
};

/// Handle of a dimension in one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DimId(usize);

/// Handle of a variable in one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VarId(usize);

/// One record of a variable, already filtered, waiting to be appended.
///
/// Produced by [`Nc4Writer::encode_record`] so that the expensive part, the
/// compression, can run on any thread while only the append itself is done
/// under the writer's lock.
#[derive(Debug)]
pub struct EncodedRecord {
    var: VarId,
    bytes: Vec<u8>,
}

impl EncodedRecord {
    /// The variable this record belongs to.
    pub fn var(&self) -> VarId {
        self.var
    }

    /// Size of the stored (filtered) record in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the record holds no bytes; never true for a real record.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

struct Dimension {
    name: String,
    len: usize,
    unlimited: bool,
}

struct Attribute {
    name: String,
    value: AttrValue,
}

enum Storage {
    Contiguous {
        address: Option<u64>,
        size: u64,
    },
    Chunked {
        chunks: Vec<ChunkEntry>,
        /// Records along the unlimited dimension, or chunk rows written.
        records: usize,
        /// Records of an unlimited variable not yet forming a full chunk.
        pending: Vec<u8>,
        pending_records: usize,
    },
}

struct Variable {
    name: String,
    dtype: DataType,
    dims: Vec<DimId>,
    /// Fixed extents; the unlimited extent (position 0) is the record count.
    shape: Vec<usize>,
    unlimited: bool,
    chunk: Option<Vec<usize>>,
    deflate: Option<u32>,
    shuffle: bool,
    fill: Vec<u8>,
    custom_fill: Option<FillValue>,
    attrs: Vec<Attribute>,
    storage: Storage,
}

impl Variable {
    fn record_elements(&self) -> usize {
        self.shape[1..].iter().product()
    }

    fn filters(&self) -> Vec<Filter> {
        let mut filters = Vec::new();
        if self.shuffle {
            filters.push(Filter::shuffle(self.dtype.size() as u32));
        }
        if let Some(level) = self.deflate {
            filters.push(Filter::deflate(level));
        }
        filters
    }

    fn apply_filters(&self, raw: Vec<u8>) -> Result<Vec<u8>> {
        let mut bytes = raw;
        if self.shuffle {
            bytes = filters::shuffle(&bytes, self.dtype.size());
        }
        if let Some(level) = self.deflate {
            bytes = filters::deflate(&bytes, level)?;
        }
        Ok(bytes)
    }
}

/// Streaming NetCDF-4 writer. See the crate documentation for the model.
pub struct Nc4Writer {
    path: PathBuf,
    out: BufWriter<File>,
    offset: u64,
    global_attrs: Vec<Attribute>,
    dims: Vec<Dimension>,
    vars: Vec<Variable>,
}

impl Nc4Writer {
    /// Create (or truncate) the file. The superblock is written as a
    /// placeholder and completed by [`finish`](Self::finish); until then the
    /// file is not a readable HDF5 file.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::create(&path)?;
        let mut out = BufWriter::with_capacity(1 << 20, file);
        out.write_all(&[0u8; hdf5::SUPERBLOCK_SIZE as usize])?;
        Ok(Self {
            path,
            out,
            offset: hdf5::SUPERBLOCK_SIZE,
            global_attrs: Vec::new(),
            dims: Vec::new(),
            vars: Vec::new(),
        })
    }

    /// The file being written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current length of a dimension. The unlimited dimension reports the
    /// records appended so far.
    pub fn dimension_len(&self, dim: DimId) -> Option<usize> {
        let dimension = self.dims.get(dim.0)?;
        if dimension.unlimited {
            return Some(
                self.vars
                    .iter()
                    .filter(|v| v.unlimited)
                    .map(|v| match &v.storage {
                        Storage::Chunked {
                            records,
                            pending_records,
                            ..
                        } => *records + *pending_records,
                        Storage::Contiguous { .. } => 0,
                    })
                    .max()
                    .unwrap_or(0),
            );
        }
        Some(dimension.len)
    }

    /// Set a global attribute, replacing one of the same name.
    pub fn put_attr(&mut self, name: &str, value: impl Into<AttrValue>) -> Result<()> {
        let value = value.into();
        check_name(name)?;
        check_attr_value(&value)?;
        put_attribute(&mut self.global_attrs, name, value);
        Ok(())
    }

    /// Add a fixed-size dimension.
    pub fn add_dimension(&mut self, name: &str, len: usize) -> Result<DimId> {
        check_name(name)?;
        if len == 0 {
            return Err(Error::Definition(format!("dimension {name} must not be empty")));
        }
        self.add_dim(name, len, false)
    }

    /// Add the unlimited dimension; a file has at most one.
    pub fn add_unlimited_dimension(&mut self, name: &str) -> Result<DimId> {
        check_name(name)?;
        if self.dims.iter().any(|d| d.unlimited) {
            return Err(Error::Definition(
                "only one unlimited dimension is supported".into(),
            ));
        }
        self.add_dim(name, 0, true)
    }

    fn add_dim(&mut self, name: &str, len: usize, unlimited: bool) -> Result<DimId> {
        if self.dims.iter().any(|d| d.name == name) {
            return Err(Error::Definition(format!("dimension {name} already exists")));
        }
        self.dims.push(Dimension {
            name: name.to_string(),
            len,
            unlimited,
        });
        Ok(DimId(self.dims.len() - 1))
    }

    /// Define a variable over `dims`. A variable named like a dimension must
    /// be that dimension's coordinate variable (one-dimensional over it).
    pub fn add_variable(
        &mut self,
        name: &str,
        dtype: DataType,
        dims: &[DimId],
        options: VariableOptions,
    ) -> Result<VarId> {
        check_name(name)?;
        if self.vars.iter().any(|v| v.name == name) {
            return Err(Error::Definition(format!("variable {name} already exists")));
        }
        for dim in dims {
            if dim.0 >= self.dims.len() {
                return Err(Error::Definition(format!(
                    "variable {name} uses a dimension of another file"
                )));
            }
        }
        if let Some((position, dim)) = self.dims.iter().enumerate().find(|(_, d)| d.name == name) {
            if dims != [DimId(position)] {
                return Err(Error::Definition(format!(
                    "variable {name} shares its name with a dimension, so it must be that dimension's coordinate variable; {}",
                    dim.name
                )));
            }
        }
        let shape: Vec<usize> = dims.iter().map(|d| self.dims[d.0].len).collect();
        let unlimited_positions: Vec<usize> = dims
            .iter()
            .enumerate()
            .filter(|(_, d)| self.dims[d.0].unlimited)
            .map(|(i, _)| i)
            .collect();
        let unlimited = match unlimited_positions.as_slice() {
            [] => false,
            [0] => true,
            _ => {
                return Err(Error::Definition(format!(
                    "variable {name}: the unlimited dimension must be the first dimension"
                )))
            }
        };

        if let Some(chunk) = &options.chunk {
            if chunk.len() != dims.len() {
                return Err(Error::Definition(format!(
                    "variable {name}: chunk shape has {} dimensions, the variable {}",
                    chunk.len(),
                    dims.len()
                )));
            }
            if chunk.contains(&0) {
                return Err(Error::Definition(format!(
                    "variable {name}: chunk extents must be positive"
                )));
            }
            if chunk.iter().any(|&c| c > u32::MAX as usize) {
                return Err(Error::Definition(format!(
                    "variable {name}: chunk extents must fit in 32 bits"
                )));
            }
            for (position, (&c, &extent)) in chunk.iter().zip(&shape).enumerate() {
                let fixed = !(unlimited && position == 0);
                if fixed && unlimited && c != extent {
                    return Err(Error::Definition(format!(
                        "variable {name}: records are appended whole, so every chunk extent but the first must equal the variable extent"
                    )));
                }
                if fixed && c > extent {
                    return Err(Error::Definition(format!(
                        "variable {name}: chunk extent {c} exceeds the dimension extent {extent}"
                    )));
                }
            }
        } else {
            if unlimited {
                return Err(Error::Definition(format!(
                    "variable {name}: a variable with the unlimited dimension must be chunked"
                )));
            }
            if options.deflate.is_some() || options.shuffle {
                return Err(Error::Definition(format!(
                    "variable {name}: filters need chunked storage"
                )));
            }
        }
        if let Some(fill) = options.fill {
            if fill.data_type() != dtype {
                return Err(Error::Definition(format!(
                    "variable {name}: the fill value type does not match {dtype:?}"
                )));
            }
        }

        let storage = if options.chunk.is_some() {
            Storage::Chunked {
                chunks: Vec::new(),
                records: 0,
                pending: Vec::new(),
                pending_records: 0,
            }
        } else {
            Storage::Contiguous {
                address: None,
                size: (shape.iter().product::<usize>() * dtype.size()) as u64,
            }
        };
        self.vars.push(Variable {
            name: name.to_string(),
            dtype,
            dims: dims.to_vec(),
            shape,
            unlimited,
            chunk: options.chunk,
            deflate: options.deflate,
            shuffle: options.shuffle,
            fill: options.fill.map_or_else(|| dtype.default_fill(), FillValue::bytes),
            custom_fill: options.fill,
            attrs: Vec::new(),
            storage,
        });
        Ok(VarId(self.vars.len() - 1))
    }

    /// Set an attribute of `var`, replacing one of the same name.
    pub fn put_var_attr(&mut self, var: VarId, name: &str, value: impl Into<AttrValue>) -> Result<()> {
        let value = value.into();
        check_name(name)?;
        check_attr_value(&value)?;
        let variable = self.var_mut(var)?;
        put_attribute(&mut variable.attrs, name, value);
        Ok(())
    }

    /// Write the whole content of a fixed-size variable at once.
    pub fn put_values<T: Element>(&mut self, var: VarId, values: &[T]) -> Result<()> {
        let variable = self.var_ref(var)?;
        if variable.unlimited {
            return Err(Error::Definition(format!(
                "variable {}: use append_record on a variable with the unlimited dimension",
                variable.name
            )));
        }
        check_element_type::<T>(variable)?;
        let expected: usize = variable.shape.iter().product();
        if values.len() != expected {
            return Err(Error::Definition(format!(
                "variable {}: {} values given, the variable holds {expected}",
                variable.name,
                values.len()
            )));
        }
        let raw = to_le_bytes(values);
        match &variable.storage {
            Storage::Contiguous { address: Some(_), .. } => Err(Error::Definition(format!(
                "variable {}: values were already written",
                variable.name
            ))),
            Storage::Contiguous { .. } => {
                let address = self.offset;
                self.write(&raw)?;
                if let Storage::Contiguous { address: slot, .. } = &mut self.vars[var.0].storage {
                    *slot = Some(address);
                }
                Ok(())
            }
            Storage::Chunked { records, .. } => {
                if *records > 0 {
                    return Err(Error::Definition(format!(
                        "variable {}: values were already written",
                        variable.name
                    )));
                }
                let chunk = variable.chunk.clone().expect("chunked storage has a chunk shape");
                let shape = variable.shape.clone();
                let element_size = variable.dtype.size();
                let fill = variable.fill.clone();
                let blocks = split_into_chunks(&raw, &shape, &chunk, element_size, &fill);
                for (offsets, block) in blocks {
                    let filtered = self.vars[var.0].apply_filters(block)?;
                    self.append_chunk(var, offsets, &filtered)?;
                }
                if let Storage::Chunked { records, .. } = &mut self.vars[var.0].storage {
                    *records = shape[0];
                }
                Ok(())
            }
        }
    }

    /// Append one record (one index along the unlimited dimension) to `var`.
    ///
    /// With a chunk extent of one along the unlimited dimension the record is
    /// filtered and written immediately; with a larger extent records are
    /// buffered until a chunk is complete.
    pub fn append_record<T: Element>(&mut self, var: VarId, values: &[T]) -> Result<()> {
        let variable = self.var_ref(var)?;
        check_record::<T>(variable, values.len())?;
        let raw = to_le_bytes(values);
        let chunk_records = variable.chunk.as_ref().expect("unlimited variables are chunked")[0];
        if chunk_records == 1 {
            let filtered = self.vars[var.0].apply_filters(raw)?;
            return self.append_encoded(var, filtered);
        }
        let variable = &mut self.vars[var.0];
        if let Storage::Chunked {
            pending,
            pending_records,
            ..
        } = &mut variable.storage
        {
            pending.extend_from_slice(&raw);
            *pending_records += 1;
            if *pending_records == chunk_records {
                self.flush_pending(var)?;
            }
        }
        Ok(())
    }

    /// Filter one record of `var` without touching the file. Only variables
    /// whose chunk extent along the unlimited dimension is one can be fed
    /// this way, since every record then is a chunk of its own.
    pub fn encode_record<T: Element>(&self, var: VarId, values: &[T]) -> Result<EncodedRecord> {
        let variable = self.var_ref(var)?;
        check_record::<T>(variable, values.len())?;
        if variable.chunk.as_ref().expect("unlimited variables are chunked")[0] != 1 {
            return Err(Error::Definition(format!(
                "variable {}: encode_record needs one record per chunk",
                variable.name
            )));
        }
        let bytes = variable.apply_filters(to_le_bytes(values))?;
        Ok(EncodedRecord { var, bytes })
    }

    /// Append a record prepared by [`encode_record`](Self::encode_record).
    pub fn append_encoded_record(&mut self, var: VarId, record: EncodedRecord) -> Result<()> {
        if record.var != var {
            return Err(Error::Definition(
                "the encoded record belongs to another variable".into(),
            ));
        }
        self.var_ref(var)?;
        self.append_encoded(var, record.bytes)
    }

    /// Write every pending structure, complete the superblock, and close.
    pub fn finish(mut self) -> Result<()> {
        for var in 0..self.vars.len() {
            self.flush_pending(VarId(var))?;
        }
        let unlimited_len = self
            .vars
            .iter()
            .filter(|v| v.unlimited)
            .map(|v| match &v.storage {
                Storage::Chunked { records, .. } => *records,
                Storage::Contiguous { .. } => 0,
            })
            .max()
            .unwrap_or(0);
        for dim in &mut self.dims {
            if dim.unlimited {
                dim.len = unlimited_len;
            }
        }
        for var in &mut self.vars {
            if var.unlimited {
                var.shape[0] = unlimited_len;
            }
        }

        let plan = self.plan_datasets();
        let heap_objects = plan.heap_objects.len();

        // Pass 1: sizes do not depend on the addresses, only on the content.
        let placeholder = Addresses {
            headers: vec![0; plan.datasets.len()],
            btrees: vec![Some(0); plan.datasets.len()],
            heap: 0,
        };
        let header_sizes: Vec<u64> = plan
            .datasets
            .iter()
            .enumerate()
            .map(|(index, dataset)| encode_dataset_header(index, dataset, &placeholder).len() as u64)
            .collect();
        let root_size = encode_root_header(&plan, &placeholder).len() as u64;

        let mut next = hdf5::align8(self.offset);
        let mut btrees = Vec::with_capacity(plan.datasets.len());
        for dataset in &plan.datasets {
            match &dataset.layout {
                LayoutPlan::Chunked { chunks, .. } if !chunks.is_empty() => {
                    btrees.push(Some(next));
                    let rank = dataset.shape.len();
                    next = hdf5::align8(next + hdf5::btree_size(chunks.len(), rank));
                }
                _ => btrees.push(None),
            }
        }
        let heap = if heap_objects > 0 {
            let address = next;
            let size = hdf5::global_heap_collection(&vec![vec![0u8; 8]; heap_objects]).len() as u64;
            next = hdf5::align8(next + size);
            address
        } else {
            UNDEFINED_ADDRESS
        };
        let mut headers = Vec::with_capacity(plan.datasets.len());
        for size in &header_sizes {
            headers.push(next);
            next = hdf5::align8(next + size);
        }
        let root = next;
        let eof = root + root_size;
        let addresses = Addresses {
            headers,
            btrees,
            heap,
        };

        // Pass 2: the real bytes, in address order.
        for (dataset, btree) in plan.datasets.iter().zip(&addresses.btrees) {
            if let (Some(address), LayoutPlan::Chunked { chunk_shape, chunks }) = (btree, &dataset.layout) {
                let nodes = hdf5::btree_v1(chunks, chunk_shape, *address);
                self.write_at(*address, &nodes)?;
            }
        }
        if heap_objects > 0 {
            let objects: Vec<Vec<u8>> = plan
                .heap_objects
                .iter()
                .map(|&dataset| addresses.headers[dataset].to_le_bytes().to_vec())
                .collect();
            self.write_at(heap, &hdf5::global_heap_collection(&objects))?;
        }
        for (index, dataset) in plan.datasets.iter().enumerate() {
            let header = encode_dataset_header(index, dataset, &addresses);
            debug_assert_eq!(header.len() as u64, header_sizes[index]);
            self.write_at(addresses.headers[index], &header)?;
        }
        let root_header = encode_root_header(&plan, &addresses);
        debug_assert_eq!(root_header.len() as u64, root_size);
        self.write_at(root, &root_header)?;

        self.out.flush()?;
        self.out.seek(SeekFrom::Start(0))?;
        self.out.write_all(&hdf5::superblock_v2(root, eof))?;
        self.out.flush()?;
        self.out.get_ref().sync_all()?;
        Ok(())
    }

    fn var_ref(&self, var: VarId) -> Result<&Variable> {
        self.vars
            .get(var.0)
            .ok_or_else(|| Error::Definition("unknown variable handle".into()))
    }

    fn var_mut(&mut self, var: VarId) -> Result<&mut Variable> {
        self.vars
            .get_mut(var.0)
            .ok_or_else(|| Error::Definition("unknown variable handle".into()))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.write_all(bytes)?;
        self.offset += bytes.len() as u64;
        Ok(())
    }

    fn write_at(&mut self, address: u64, bytes: &[u8]) -> Result<()> {
        debug_assert!(address >= self.offset, "metadata is written in address order");
        let padding = (address - self.offset) as usize;
        if padding > 0 {
            self.write(&vec![0u8; padding])?;
        }
        self.write(bytes)
    }

    /// Append one already-filtered record as the next chunk of `var`.
    fn append_encoded(&mut self, var: VarId, bytes: Vec<u8>) -> Result<()> {
        let variable = &self.vars[var.0];
        let rank = variable.shape.len();
        let records = match &variable.storage {
            Storage::Chunked { pending_records, .. } if *pending_records > 0 => {
                return Err(Error::Definition(format!(
                    "variable {}: encoded records cannot be mixed with buffered ones",
                    variable.name
                )))
            }
            Storage::Chunked { records, .. } => *records,
            Storage::Contiguous { .. } => unreachable!("records go to chunked variables"),
        };
        let mut offsets = vec![0u64; rank];
        offsets[0] = records as u64;
        self.append_chunk(var, offsets, &bytes)?;
        if let Storage::Chunked { records, .. } = &mut self.vars[var.0].storage {
            *records += 1;
        }
        Ok(())
    }

    fn append_chunk(&mut self, var: VarId, offsets: Vec<u64>, bytes: &[u8]) -> Result<()> {
        let size = u32::try_from(bytes.len()).map_err(|_| {
            Error::Definition(format!(
                "variable {}: a stored chunk must be smaller than 4 GiB",
                self.vars[var.0].name
            ))
        })?;
        let address = self.offset;
        self.write(bytes)?;
        if let Storage::Chunked { chunks, .. } = &mut self.vars[var.0].storage {
            chunks.push(ChunkEntry {
                offsets,
                address,
                size,
            });
        }
        Ok(())
    }

    /// Write the buffered records of an unlimited variable as one chunk,
    /// padded with the fill value when the chunk is not complete.
    fn flush_pending(&mut self, var: VarId) -> Result<()> {
        let variable = &mut self.vars[var.0];
        let (pending, pending_records, records) = match &mut variable.storage {
            Storage::Chunked {
                pending,
                pending_records,
                records,
                ..
            } if *pending_records > 0 => (
                std::mem::take(pending),
                std::mem::take(pending_records),
                *records,
            ),
            _ => return Ok(()),
        };
        let chunk_records = variable.chunk.as_ref().expect("unlimited variables are chunked")[0];
        let mut raw = pending;
        let missing = (chunk_records - pending_records) * variable.record_elements();
        raw.reserve(missing * variable.fill.len());
        for _ in 0..missing {
            raw.extend_from_slice(&variable.fill);
        }
        let filtered = variable.apply_filters(raw)?;
        let mut offsets = vec![0u64; variable.shape.len()];
        offsets[0] = records as u64;
        self.append_chunk(var, offsets, &filtered)?;
        if let Storage::Chunked { records, .. } = &mut self.vars[var.0].storage {
            *records += pending_records;
        }
        Ok(())
    }

    /// Turn the definitions into the HDF5 datasets to emit: the variables in
    /// definition order, then one hidden dimension scale for every dimension
    /// without a coordinate variable.
    fn plan_datasets(&self) -> FilePlan {
        let coordinate_of: Vec<Option<usize>> = self
            .vars
            .iter()
            .map(|var| {
                self.dims
                    .iter()
                    .position(|d| d.name == var.name)
                    .filter(|&dim| var.dims == [DimId(dim)])
            })
            .collect();
        let mut scale_dataset: Vec<usize> = vec![usize::MAX; self.dims.len()];
        for (index, coordinate) in coordinate_of.iter().enumerate() {
            if let Some(dim) = coordinate {
                scale_dataset[*dim] = index;
            }
        }
        let mut hidden_scales = Vec::new();
        for (dim, slot) in scale_dataset.iter_mut().enumerate() {
            if *slot == usize::MAX {
                *slot = self.vars.len() + hidden_scales.len();
                hidden_scales.push(dim);
            }
        }

        let reference_list = |dim: usize| -> Vec<(usize, u32)> {
            self.vars
                .iter()
                .enumerate()
                .filter(|(index, _)| coordinate_of[*index] != Some(dim))
                .flat_map(|(index, var)| {
                    var.dims
                        .iter()
                        .enumerate()
                        .filter(move |(_, d)| d.0 == dim)
                        .map(move |(position, _)| (index, position as u32))
                })
                .collect()
        };

        let mut heap_objects = Vec::new();
        let mut datasets = Vec::with_capacity(self.vars.len() + hidden_scales.len());
        for (index, var) in self.vars.iter().enumerate() {
            let mut attrs = Vec::new();
            if let Some(dim) = coordinate_of[index] {
                attrs.extend(scale_attributes(&self.dims[dim].name, dim));
                let entries = reference_list(dim);
                if !entries.is_empty() {
                    attrs.push(AttrPlan::ReferenceList { entries });
                }
            } else if !var.dims.is_empty() {
                let first_heap_object = heap_objects.len() + 1;
                for dim in &var.dims {
                    heap_objects.push(scale_dataset[dim.0]);
                }
                attrs.push(AttrPlan::DimensionList {
                    rank: var.dims.len(),
                    first_heap_object,
                });
            }
            if let Some(fill) = var.custom_fill {
                attrs.push(inline_attribute("_FillValue", &fill.attr_value()));
            }
            for attr in &var.attrs {
                attrs.push(inline_attribute(&attr.name, &attr.value));
            }

            let shape: Vec<u64> = var.shape.iter().map(|&s| s as u64).collect();
            let mut max_shape = shape.clone();
            if var.unlimited {
                max_shape[0] = UNDEFINED_ADDRESS;
            }
            let (layout, alloc_time) = match &var.storage {
                Storage::Contiguous { address, size } => (
                    LayoutPlan::Contiguous {
                        address: address.unwrap_or(UNDEFINED_ADDRESS),
                        size: *size,
                    },
                    hdf5::ALLOC_TIME_LATE,
                ),
                Storage::Chunked { chunks, .. } => (
                    LayoutPlan::Chunked {
                        chunk_shape: var
                            .chunk
                            .as_ref()
                            .expect("chunked storage has a chunk shape")
                            .iter()
                            .map(|&c| c as u64)
                            .collect(),
                        chunks: chunks.clone(),
                    },
                    hdf5::ALLOC_TIME_INCREMENTAL,
                ),
            };
            datasets.push(DatasetPlan {
                name: var.name.clone(),
                shape,
                max_shape,
                datatype: var.dtype.datatype_message(),
                element_size: var.dtype.size() as u32,
                fill: var.fill.clone(),
                alloc_time,
                filters: var.filters(),
                layout,
                attrs,
            });
        }

        for dim in hidden_scales {
            let dimension = &self.dims[dim];
            let mut attrs = scale_attributes(
                &format!(
                    "This is a netCDF dimension but not a netCDF variable. {:10}",
                    dimension.len
                ),
                dim,
            );
            let entries = reference_list(dim);
            if !entries.is_empty() {
                attrs.push(AttrPlan::ReferenceList { entries });
            }
            let len = dimension.len as u64;
            let (layout, alloc_time, max_shape) = if dimension.unlimited {
                (
                    LayoutPlan::Chunked {
                        chunk_shape: vec![len.clamp(1, 1024)],
                        chunks: Vec::new(),
                    },
                    hdf5::ALLOC_TIME_INCREMENTAL,
                    vec![UNDEFINED_ADDRESS],
                )
            } else {
                (
                    LayoutPlan::Contiguous {
                        address: UNDEFINED_ADDRESS,
                        size: len * 4,
                    },
                    hdf5::ALLOC_TIME_LATE,
                    vec![len],
                )
            };
            datasets.push(DatasetPlan {
                name: dimension.name.clone(),
                shape: vec![len],
                max_shape,
                datatype: DataType::I32.datatype_message(),
                element_size: 4,
                fill: DataType::I32.default_fill(),
                alloc_time,
                filters: Vec::new(),
                layout,
                attrs,
            });
        }

        let mut root_attrs = vec![inline_attribute(
            "_NCProperties",
            &AttrValue::Str(format!(
                "version=2,nc4-stream={}",
                env!("CARGO_PKG_VERSION")
            )),
        )];
        root_attrs.extend(
            self.global_attrs
                .iter()
                .map(|attr| inline_attribute(&attr.name, &attr.value)),
        );

        FilePlan {
            datasets,
            root_attrs,
            heap_objects,
        }
    }
}

struct FilePlan {
    datasets: Vec<DatasetPlan>,
    root_attrs: Vec<AttrPlan>,
    /// Dataset index every global heap object points at, in heap order.
    heap_objects: Vec<usize>,
}

struct DatasetPlan {
    name: String,
    shape: Vec<u64>,
    max_shape: Vec<u64>,
    datatype: Vec<u8>,
    element_size: u32,
    fill: Vec<u8>,
    alloc_time: u8,
    filters: Vec<Filter>,
    layout: LayoutPlan,
    attrs: Vec<AttrPlan>,
}

enum LayoutPlan {
    Contiguous { address: u64, size: u64 },
    Chunked { chunk_shape: Vec<u64>, chunks: Vec<ChunkEntry> },
}

enum AttrPlan {
    Inline {
        name: String,
        datatype: Vec<u8>,
        dataspace: Vec<u8>,
        raw: Vec<u8>,
    },
    /// `REFERENCE_LIST` of a dimension scale: (dataset index, dimension position).
    ReferenceList { entries: Vec<(usize, u32)> },
    /// `DIMENSION_LIST` of a data variable: `rank` heap objects from `first_heap_object` (1-based).
    DimensionList { rank: usize, first_heap_object: usize },
}

/// Where everything lands; placeholders during the sizing pass.
struct Addresses {
    headers: Vec<u64>,
    btrees: Vec<Option<u64>>,
    heap: u64,
}

fn encode_attr(attr: &AttrPlan, addresses: &Addresses) -> Vec<u8> {
    match attr {
        AttrPlan::Inline {
            name,
            datatype,
            dataspace,
            raw,
        } => hdf5::attribute(name, datatype, dataspace, raw),
        AttrPlan::ReferenceList { entries } => {
            let mut raw = Vec::with_capacity(entries.len() * hdf5::REFERENCE_LIST_ENTRY_SIZE);
            for (dataset, dimension) in entries {
                raw.extend_from_slice(&hdf5::reference_list_entry(
                    addresses.headers[*dataset],
                    *dimension,
                ));
            }
            hdf5::attribute(
                "REFERENCE_LIST",
                &hdf5::datatype_reference_list(),
                &hdf5::dataspace(&[entries.len() as u64], None),
                &raw,
            )
        }
        AttrPlan::DimensionList {
            rank,
            first_heap_object,
        } => {
            let mut raw = Vec::with_capacity(rank * 16);
            for position in 0..*rank {
                raw.extend_from_slice(&hdf5::vlen_reference(
                    1,
                    addresses.heap,
                    (first_heap_object + position) as u32,
                ));
            }
            hdf5::attribute(
                "DIMENSION_LIST",
                &hdf5::datatype_vlen_sequence(&hdf5::datatype_object_reference()),
                &hdf5::dataspace(&[*rank as u64], None),
                &raw,
            )
        }
    }
}

/// The object header of the dataset at `index` in the plan.
fn encode_dataset_header(index: usize, dataset: &DatasetPlan, addresses: &Addresses) -> Vec<u8> {
    let mut messages = vec![
        Message {
            type_id: hdf5::MSG_DATASPACE,
            payload: hdf5::dataspace(&dataset.shape, Some(&dataset.max_shape)),
        },
        Message {
            type_id: hdf5::MSG_DATATYPE,
            payload: dataset.datatype.clone(),
        },
        Message {
            type_id: hdf5::MSG_FILL_VALUE,
            payload: hdf5::fill_value(&dataset.fill, dataset.alloc_time),
        },
    ];
    if !dataset.filters.is_empty() {
        messages.push(Message {
            type_id: hdf5::MSG_FILTER_PIPELINE,
            payload: hdf5::filter_pipeline(&dataset.filters),
        });
    }
    let layout = match &dataset.layout {
        LayoutPlan::Contiguous { address, size } => hdf5::layout_contiguous(*address, *size),
        LayoutPlan::Chunked { chunk_shape, chunks } => {
            let btree = if chunks.is_empty() {
                UNDEFINED_ADDRESS
            } else {
                addresses.btrees[index].unwrap_or(UNDEFINED_ADDRESS)
            };
            hdf5::layout_chunked(btree, chunk_shape, dataset.element_size)
        }
    };
    messages.push(Message {
        type_id: hdf5::MSG_DATA_LAYOUT,
        payload: layout,
    });
    for attr in &dataset.attrs {
        messages.push(Message {
            type_id: hdf5::MSG_ATTRIBUTE,
            payload: encode_attr(attr, addresses),
        });
    }
    hdf5::object_header_v2(&messages)
}

fn encode_root_header(plan: &FilePlan, addresses: &Addresses) -> Vec<u8> {
    let mut messages = vec![
        Message {
            type_id: hdf5::MSG_LINK_INFO,
            payload: hdf5::link_info(),
        },
        Message {
            type_id: hdf5::MSG_GROUP_INFO,
            payload: hdf5::group_info(),
        },
    ];
    for attr in &plan.root_attrs {
        messages.push(Message {
            type_id: hdf5::MSG_ATTRIBUTE,
            payload: encode_attr(attr, addresses),
        });
    }
    for (dataset, address) in plan.datasets.iter().zip(&addresses.headers) {
        messages.push(Message {
            type_id: hdf5::MSG_LINK,
            payload: hdf5::hard_link(&dataset.name, *address),
        });
    }
    hdf5::object_header_v2(&messages)
}

/// The attributes the HDF5 dimension scale API puts on a scale dataset.
fn scale_attributes(name: &str, dim: usize) -> Vec<AttrPlan> {
    vec![
        AttrPlan::Inline {
            name: "CLASS".into(),
            datatype: hdf5::datatype_fixed_string("DIMENSION_SCALE".len() as u32 + 1),
            dataspace: hdf5::dataspace(&[], None),
            raw: b"DIMENSION_SCALE\0".to_vec(),
        },
        AttrPlan::Inline {
            name: "NAME".into(),
            datatype: hdf5::datatype_fixed_string(name.len() as u32 + 1),
            dataspace: hdf5::dataspace(&[], None),
            raw: {
                let mut raw = name.as_bytes().to_vec();
                raw.push(0);
                raw
            },
        },
        AttrPlan::Inline {
            name: "_Netcdf4Dimid".into(),
            datatype: hdf5::datatype_fixed_point(4, true),
            dataspace: hdf5::dataspace(&[], None),
            raw: (dim as i32).to_le_bytes().to_vec(),
        },
    ]
}

/// A user attribute: strings are fixed-length scalars of exactly their
/// length, numbers are one-dimensional arrays, as netcdf-c stores them.
fn inline_attribute(name: &str, value: &AttrValue) -> AttrPlan {
    let (datatype, dataspace, raw) = match value {
        AttrValue::Str(text) => {
            let raw = if text.is_empty() {
                vec![0]
            } else {
                text.as_bytes().to_vec()
            };
            (
                hdf5::datatype_fixed_string(raw.len() as u32),
                hdf5::dataspace(&[], None),
                raw,
            )
        }
        AttrValue::F32(v) => numeric_attribute(DataType::F32, &[*v]),
        AttrValue::F64(v) => numeric_attribute(DataType::F64, &[*v]),
        AttrValue::I32(v) => numeric_attribute(DataType::I32, &[*v]),
        AttrValue::I64(v) => numeric_attribute(DataType::I64, &[*v]),
        AttrValue::U32(v) => numeric_attribute(DataType::U32, &[*v]),
        AttrValue::U64(v) => (
            hdf5::datatype_fixed_point(8, false),
            hdf5::dataspace(&[1], None),
            v.to_le_bytes().to_vec(),
        ),
        AttrValue::F32s(v) => numeric_attribute(DataType::F32, v),
        AttrValue::F64s(v) => numeric_attribute(DataType::F64, v),
        AttrValue::I32s(v) => numeric_attribute(DataType::I32, v),
        AttrValue::I64s(v) => numeric_attribute(DataType::I64, v),
    };
    AttrPlan::Inline {
        name: name.to_string(),
        datatype,
        dataspace,
        raw,
    }
}

fn numeric_attribute<T: Element>(dtype: DataType, values: &[T]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        dtype.datatype_message(),
        hdf5::dataspace(&[values.len() as u64], None),
        to_le_bytes(values),
    )
}

fn put_attribute(attrs: &mut Vec<Attribute>, name: &str, value: AttrValue) {
    if let Some(existing) = attrs.iter_mut().find(|a| a.name == name) {
        existing.value = value;
    } else {
        attrs.push(Attribute {
            name: name.to_string(),
            value,
        });
    }
}

fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('/') || name.contains('\0') {
        return Err(Error::Definition(format!(
            "{name:?} is not a valid NetCDF name"
        )));
    }
    Ok(())
}

fn check_attr_value(value: &AttrValue) -> Result<()> {
    let empty = match value {
        AttrValue::F32s(v) => v.is_empty(),
        AttrValue::F64s(v) => v.is_empty(),
        AttrValue::I32s(v) => v.is_empty(),
        AttrValue::I64s(v) => v.is_empty(),
        _ => false,
    };
    if empty {
        return Err(Error::Definition("attribute arrays must not be empty".into()));
    }
    Ok(())
}

fn check_element_type<T: Element>(variable: &Variable) -> Result<()> {
    if T::DATA_TYPE != variable.dtype {
        return Err(Error::Definition(format!(
            "variable {} is {:?}, values are {:?}",
            variable.name,
            variable.dtype,
            T::DATA_TYPE
        )));
    }
    Ok(())
}

fn check_record<T: Element>(variable: &Variable, len: usize) -> Result<()> {
    if !variable.unlimited {
        return Err(Error::Definition(format!(
            "variable {} has no unlimited dimension; use put_values",
            variable.name
        )));
    }
    check_element_type::<T>(variable)?;
    let expected = variable.record_elements();
    if len != expected {
        return Err(Error::Definition(format!(
            "variable {}: a record holds {expected} values, {len} given",
            variable.name
        )));
    }
    Ok(())
}

fn to_le_bytes<T: Element>(values: &[T]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * T::DATA_TYPE.size());
    for value in values {
        value.push_le(&mut bytes);
    }
    bytes
}

/// Cut a row-major array into its chunks, each padded with `fill` where it
/// reaches past the array. Returns (element offsets, raw chunk bytes) in
/// row-major chunk order.
fn split_into_chunks(
    raw: &[u8],
    shape: &[usize],
    chunk: &[usize],
    element_size: usize,
    fill: &[u8],
) -> Vec<(Vec<u64>, Vec<u8>)> {
    let rank = shape.len();
    let grid: Vec<usize> = shape
        .iter()
        .zip(chunk)
        .map(|(&extent, &c)| extent.div_ceil(c))
        .collect();
    let chunk_elements: usize = chunk.iter().product();
    let mut blocks = Vec::with_capacity(grid.iter().product());
    for_each_index(&grid, |position| {
        let mut block = Vec::with_capacity(chunk_elements * element_size);
        for_each_index(chunk, |local| {
            let mut flat = 0;
            let mut inside = true;
            for d in 0..rank {
                let global = position[d] * chunk[d] + local[d];
                if global >= shape[d] {
                    inside = false;
                    break;
                }
                flat = flat * shape[d] + global;
            }
            if inside {
                block.extend_from_slice(&raw[flat * element_size..(flat + 1) * element_size]);
            } else {
                block.extend_from_slice(fill);
            }
        });
        let offsets = position
            .iter()
            .zip(chunk)
            .map(|(&p, &c)| (p * c) as u64)
            .collect();
        blocks.push((offsets, block));
    });
    blocks
}

/// Call `f` with every index of a row-major grid of `dims`, in order.
fn for_each_index(dims: &[usize], mut f: impl FnMut(&[usize])) {
    if dims.contains(&0) {
        return;
    }
    let mut index = vec![0usize; dims.len()];
    loop {
        f(&index);
        let mut d = dims.len();
        loop {
            if d == 0 {
                return;
            }
            d -= 1;
            index[d] += 1;
            if index[d] < dims[d] {
                break;
            }
            index[d] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_are_cut_in_row_major_order_and_padded() {
        // 3 x 4 array of u8 with 2 x 3 chunks: a 2 x 2 chunk grid.
        let raw: Vec<u8> = (0..12).collect();
        let fill = [255u8];
        let blocks = split_into_chunks(&raw, &[3, 4], &[2, 3], 1, &fill);
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].0, vec![0, 0]);
        assert_eq!(blocks[0].1, vec![0, 1, 2, 4, 5, 6]);
        assert_eq!(blocks[1].0, vec![0, 3]);
        assert_eq!(blocks[1].1, vec![3, 255, 255, 7, 255, 255]);
        assert_eq!(blocks[2].0, vec![2, 0]);
        assert_eq!(blocks[2].1, vec![8, 9, 10, 255, 255, 255]);
        assert_eq!(blocks[3].0, vec![2, 3]);
        assert_eq!(blocks[3].1, vec![11, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn index_iteration_covers_the_grid_once() {
        let mut seen = Vec::new();
        for_each_index(&[2, 3], |index| seen.push(index.to_vec()));
        assert_eq!(seen.len(), 6);
        assert_eq!(seen[0], [0, 0]);
        assert_eq!(seen[1], [0, 1]);
        assert_eq!(seen[5], [1, 2]);
        let mut none = 0;
        for_each_index(&[2, 0], |_| none += 1);
        assert_eq!(none, 0);
    }
}
