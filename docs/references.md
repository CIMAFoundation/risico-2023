# References

The scientific publications behind the models in `risico-2023`.

## RISICO

- Fiorucci, P., Gaetani, F., Minciardi, R. (2008). Development and application of a system for
  dynamic wildfire risk assessment in Italy. *Environmental Modelling & Software*, 23(6),
  690–702. [doi:10.1016/j.envsoft.2007.05.008](https://doi.org/10.1016/j.envsoft.2007.05.008)

    The original description of RISICO: the fuel moisture and fire spread models, run on every
    grid cell at every forecast step, from limited-area weather forecasts and maps of
    vegetation and terrain.

- Perello, N., Trucchia, A., D'Andrea, M., Meschi, G., Ghasemiazma, F., Degli Esposti, S.,
  Fiorucci, P., Ferraris, L., Gollini, A., Negro, D. (2025). Fuel-aware forest fire danger
  rating system RISICO: a comparative study for Italy. *International Journal of Wildland Fire*,
  34, WF25023. [doi:10.1071/WF25023](https://doi.org/10.1071/WF25023) (open access)

    The current version of the model, with fuel classes derived from a machine-learning
    wildfire susceptibility map, compared with other fire danger indices on 16 years of
    Italian wildfires.

## Canadian Fire Weather Index System

- Van Wagner, C.E. (1987). *Development and structure of the Canadian Forest Fire Weather Index
  System*. Forestry Technical Report 35, Canadian Forestry Service, Ottawa.
- Van Wagner, C.E., Pickett, T.L. (1985). *Equations and FORTRAN program for the Canadian
  Forest Fire Weather Index System*. Forestry Technical Report 33, Canadian Forestry Service,
  Ottawa.
- Wang, X., Wotton, B.M., Cantin, A.S., Parisien, M.-A., Anderson, K., Moore, B.,
  Flannigan, M.D. (2017). cffdrs: an R package for the Canadian Forest Fire Danger Rating
  System. *Ecological Processes*, 6, 5.
  [doi:10.1186/s13717-017-0070-z](https://doi.org/10.1186/s13717-017-0070-z) — the reference
  implementation the `cffdrs` model version follows
  ([CRAN package](https://cran.r-project.org/package=cffdrs)).

## Other indices

Keetch–Byram Drought Index (`KBDI`)
:   Keetch, J.J., Byram, G.M. (1968). *A drought index for forest fire control*. Research Paper
    SE-38, USDA Forest Service, Southeastern Forest Experiment Station, Asheville, NC.

McArthur Forest Fire Danger Index, Mark 5 (`Mark5`)
:   Noble, I.R., Bary, G.A.V., Gill, A.M. (1980). McArthur's fire-danger meters expressed as
    equations. *Australian Journal of Ecology*, 5, 201–203.
    [doi:10.1111/j.1442-9993.1980.tb01243.x](https://doi.org/10.1111/j.1442-9993.1980.tb01243.x)

Angström index (`Angstrom`)
:   Chandler, C., Cheney, P., Thomas, P., Trabaud, L., Williams, D. (1983). *Fire in Forestry,
    Volume 1: Forest Fire Behavior and Effects*. John Wiley & Sons, New York.

Fosberg Fire Weather Index (`Fosberg`)
:   Fosberg, M.A. (1978). Weather in wildland fire management: the fire weather index.
    *Proceedings of the Conference on Sierra Nevada Meteorology*, Lake Tahoe, CA, 19–21
    January 1978.

Nesterov ignition index (`Nesterov`)
:   Nesterov, V.G. (1949). *Combustibility of the forest and methods for its determination*
    (in Russian). Goslesbumizdat, Moscow.

Sharples fuel moisture index (`Sharples`)
:   Sharples, J.J., McRae, R.H.D., Weber, R.O., Gill, A.M. (2009). A simple index for assessing
    fuel moisture content. *Environmental Modelling & Software*, 24(5), 637–646.
    [doi:10.1016/j.envsoft.2008.10.012](https://doi.org/10.1016/j.envsoft.2008.10.012)

Orieux water reserve index (`Orieux`)
:   Orieux, A. (1974). Water reserve index for forest fire danger in Mediterranean France,
    as listed in the [comparison of fire danger indices](https://wikifire.wsl.ch/table_indicesf861.pdf?fileId=15)
    of the WSL fire database.

Hot–Dry–Windy Index (`Hdw`)
:   Srock, A.F., Charney, J.J., Potter, B.E., Goodrick, S.L. (2018). The Hot-Dry-Windy Index:
    a new fire weather index. *Atmosphere*, 9(7), 279.
    [doi:10.3390/atmos9070279](https://doi.org/10.3390/atmos9070279)
