# Adding necessary build targets

Compiling to your own system works by default and without 

## Compiling to Linux from Windows

Make sure you have MinGW installed.

Run the following in a command line to install the build target:

    rustup target add x86_64-unknown-linux-gnu

## Compiling to Windows from Linux

On debian-based distros, run the following commands to set up the Windows-GNU target:

    sudo apt install mingw-w64
    rustup target add x86_64-pc-windows-gnu

Then run

    cargo build --release --target x86_64-pc-windows-gnu

or execute `build_release.sh` to build. The executable will be in `target/x86_64-pc-windows-gnu/nw-tex.exe`.

