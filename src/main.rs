use std::{path::{Path, PathBuf}, fs};

use anyhow::{Result, Error};
use clap::{Parser, ValueEnum, ArgAction};
use nw_tex::CgfxFileRegistry;

#[derive(Debug, Clone, ValueEnum)]
enum Method {
    /// Takes in a 'kdm_*.bin' file and disassembles it into a human-readable '*.kersti' file
    Extract,
    /// Takes in your modified kersti file and builds it into the original game file
    Rebuild,
}

#[derive(Parser, Debug)]
#[command(author, version, long_about = None, disable_version_flag = true, disable_help_flag = true)]
struct Args {
    /// Whether to 'disassemble' a KDM source file into a readable Kersti file or 'build' it vice versa.
    method: Method,
    
    /// The input 'XXX_xx.bin' file to extract or .yaml file to build to the source asset again.
    /// 
    /// Examples: (extract) EUR_en.bin, EUR_de.bin (rebuild) EUR_en_tex.yaml, EUR_it_tex.yaml
    /// 
    /// If the method is `extract`, then there needs to be an adjacent file with the same name  
    /// but ending on '_info.bin'. This file contains important metadata on the structure of the
    /// archive.
    #[arg(verbatim_doc_comment)]
    input: String,
    
    /// The output file. If input is a KDM file, output should be '*.kersti' and if it is a Kersti file, it should be a KDM file.
    /// Will be inferred by the input file by default but can be set manually like this.
    #[arg(short, long, verbatim_doc_comment)]
    output: Option<String>,
    
    /// Print app version
    #[arg(short, long, action = ArgAction::Version)]
    version: Option<bool>,
    
    // makes clap always display the long help even if you specify -h
    // that's because the short help just does not give you enough information to operate
    // and it's uninutitive that there is a difference between two versions of the same flag
    
    /// Print help
    #[arg(short, long, action = ArgAction::HelpLong)]
    help: Option<bool>,
}

fn get_input_sibling_path(input: &Path, new_file_ending: &str) -> Result<PathBuf> {
    let mut path_buf = input.parent()
        .ok_or_else(|| Error::msg("Could not find containing directory of input file."))?
        .to_owned();
    
    let input_file_name = input.file_name()
        .ok_or_else(|| Error::msg("Invalid file name for input."))?
        .to_str()
        .ok_or_else(|| Error::msg("Input file name contains invalid (not utf8) characters."))?;
    
    let mut output_file_name = if input_file_name.ends_with(".bin") {
        input_file_name[..input_file_name.len() - 4].to_owned()
    } else {
        input_file_name.to_owned()
    };
    
    output_file_name.push_str(new_file_ending);
    path_buf.push(output_file_name);
    
    Ok(path_buf)
}

fn disassemble(input: PathBuf, opt_output: Option<String>) -> Result<()> {
    let secondary_input = get_input_sibling_path(&input, "_info.bin")?;
    
    let output_name = match opt_output {
        Some(path) => PathBuf::from(path),
        None => get_input_sibling_path(&input, "_tex.yaml")?,
    };
    
    println!("secondary input path: {}", secondary_input.display());
    println!("disasm output path: {}", output_name.display());
    
    let secondary_file_buf = fs::read(&secondary_input)
        .expect(&format!("Could not open file {}. Make sure `input` has an adjacent \
file with the same name but ending on '_info.bin' rather than '.bin'", secondary_input.display()));
    
    let registry = CgfxFileRegistry::new(&secondary_file_buf)?;
    
    fs::write(output_name, registry.to_yaml()?)?;
    
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("{:?}\n", args);
    
    let input = Path::new(&args.input).to_owned();
    let output = args.output;
    
    match args.method {
        Method::Extract => disassemble(input, output)?,
        Method::Rebuild => todo!(),
    }
    
    Ok(())
}
