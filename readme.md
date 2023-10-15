# NW-Tex

A tool to extract and rebuild Paper Mario: Sticker Star's localized texture archives

## Usage

To use this, you need a full dump of Paper Mario: Sticker Star's RomFS. You should also have a way of decompressing .bcrez files to .bcres files (for example, [blz.exe from CUE's DS/GBA Compressors](https://gbatemp.net/threads/nintendo-ds-gba-compressors.313278/)) and a way to open and edit .bcres files (for example, [KillzXGaming's CTR Studio](https://github.com/MapStudioProject/CTR-Studio)) in order to use the files this program will output.

That being said, navigate to `Lang/` (textures containing localized text) or `NWTexture/` (generic textures like flowers or stickers that for some reason are still localized) in the romfs and find the `XXX_xx.bin` file for the language you want, for example, `EUR_en.bin`.

With this file, you can extract its contents like this:

    nw-tex extract <input file you just chose>

(command above used in a command line, e.g., Command Prompt)

The program will extract a table of all files from the archive into a similarly named file ending on `_tex.yaml` and the actual files in the archive into the folder with the same name as this. (You can also determine the place where these will be placed and under which name by adding `--output <output file ending on .yaml` to the command).

Always make sure that next to the file you are passing in as input, there exists a similarly named file who's file name ends on `_info`, for example, `EUR_en_info.bin`. The program will find this file automatically, **but make sure to not separate the two files or rename just one of them**.

Now you can use the files in `XXX_xx_tex/` (whatever name of the input file you chose) like you do with any other .bcrez file in the game. If you do not know what to do with them, you can check out [this tutorial by Hunter Xuman](https://gamebanana.com/tuts/15568).

To rebuild the archive back into its original two files (WHICH IS CURRENTLY NOT SUPPORTED YET!), you can run

    nw-tex rebuild <name of file ending on [...]_tex.yaml> --output <name of new .bin file>

Make sure to pass a name to `--output` that is not the file name of the original file, so you do not overwrite it, in case you want to it again.

## Installation

<!-- Download the latest release from <https://github.com/Darxoon/nw-tex/releases> -->
When a release is made, you can download it from the releases section. Until then, you can only compile it.

## Compilation

Clone the repository with `git clone https://github.com/Darxoon/nw-tex` into a convenient location. Make sure you have [the Rust toolchain](https://www.rust-lang.org/tools/install) installed. 

Navigate into the folder `nw-tex/` with a Shell or Command Prompt and use cargo to run or build the program.

To build it in a way that you can distribute it, run `cargo build --release`. When it is done loading, you can head to `target/release` and copy `nw-tex.exe` (or just `nw-tex` if you are on linux) somewhere else so you can use it or distribute it.

## Contact

For any discussions or help regarding Paper Mario: Sticker Star or this program, join the Modern Paper Mario Modding server (<https://discord.gg/9EzRrfVfPg>) or the Star Haven server (<https://discord.gg/Pj4u7wB>).

You can also raise an Issue here on Github or contact me directly (my discord username is @darxoon).
