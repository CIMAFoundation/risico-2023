Before we build the application, we’ll also need to install a linker for the target platform. Fortunately, the musl-cross tap from Homebrew provides a complete cross-compilation toolchain for Mac OS.

$ brew install filosottile/musl-cross/musl-cross
Bash
Now we need to inform Cargo that our project uses the newly-installed linker when building for the x86_64-unknown-linux-musl platform. Create a new directory called .cargo in your project folder and a new file called config inside the new folder.

$ mkdir .cargo
$ echo '[target.x86_64-unknown-linux-musl]
linker = "x86_64-linux-musl-gcc"' > .cargo/config
Bash
On my system, some of the dependencies did not pick up the configured linker automatically and tried to use musl-gcc anyway. To get around this quickly, I simply created a symlink to the new linker:

$ ln -s /usr/local/bin/x86_64-linux-musl-gcc /usr/local/bin/musl-gcc
Bash
With the new target platform for the compiler installed and configured, we can now have Cargo cross-compile!