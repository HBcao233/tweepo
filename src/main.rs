mod apply;
mod po;
mod scanner;
mod types;
mod utils;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::exit;

use crate::apply::plan_apply;
use crate::po::parse_po;
use crate::scanner::Scanner;
use crate::utils::po_escape;

const NC: &str = "\x1b[0m";
const RED: &str = "\x1b[1;31m";
const GREEN: &str = "\x1b[1;32m";
const BLUE: &str = "\x1b[1;34m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[1;33m";

fn main() {
    let mut args = env::args();
    args.next();

    let subcommand = args.next();
    let subcommand = match subcommand.as_deref() {
        Some(v) if matches!(v, "gettext" | "check" | "apply") => v,
        _ => {
            println!("{GREEN}Usage{NC}: tweepo <gettext|check|apply>");
            exit(1);
        }
    };

    let mut input = Vec::new();
    let mut output = None;

    let mut help = false;
    let mut recursive = false;

    while let Some(arg) = args.next() {
        if arg.starts_with('-') {
            if arg == "-h" || arg == "--help" {
                help = true;
                break;
            } else if arg == "-o" || arg == "--output" {
                output = Some(args.next().unwrap_or_else(|| {
                    eprintln!("{RED}error{NC}: \"{arg}\" must be followed by a value.");
                    exit(1);
                }));
            } else if arg == "-r" || arg == "--recursive" {
                recursive = true;
            } else {
                println!("{YELLOW}warn{NC}: unknown option \"{arg}\"");
            }
        } else {
            input.push(arg);
        }
    }

    if input.is_empty() || help {
        match subcommand {
            "gettext" => println!(
                "\
{GREEN}Usage{NC}: tweepo gettext [Options] <input>

{BLUE}Options{NC}:
  -o, --output           output path
  -h, --help             show this message
  -r, --recursive        process directory recursively
"
            ),
            "check" => println!(
                "\
{GREEN}Usage{NC}: tweepo check <twee> <po>
"
            ),
            "apply" => println!(
                "\
{GREEN}Usage{NC}: tweepo apply [Options] <input>

{BLUE}Options{NC}:
  -o, --output           output path
  -h, --help             show this message
  -r, --recursive        process directory recursively
"
            ),
            _ => unreachable!(),
        }
        exit(!help as i32);
    }

    match subcommand {
        "gettext" => {
            if input.is_empty() {
                eprintln!(
                    "{RED}error{NC}: gettext requires <twee>; got {} positional argument(s)",
                    input.len()
                );
                exit(1);
            }

            let output = output.unwrap_or_else(|| {
                eprintln!("{RED}error{NC}: output path must be provided, use \"-o path\" to pass.");
                exit(1);
            });

            if let Err(e) = gettext(&input[0], &output, recursive) {
                eprintln!("{RED}error{NC}: {}", e);
                exit(1);
            }
        }
        "check" => {
            if input.len() < 2 {
                eprintln!(
                    "{RED}error{NC}: check requires <twee> <po>; got {} positional argument(s)",
                    input.len()
                );
                exit(1);
            }

            if let Err(e) = apply(&input[0], &input[1], None, true) {
                eprintln!("{RED}error{NC}: {}", e);
                exit(1);
            }
        }
        "apply" => {
            if input.len() < 2 {
                eprintln!(
                    "{RED}error{NC}: apply requires <twee> <po>; got {} positional argument(s)",
                    input.len()
                );
                exit(1);
            }
            let output = output.unwrap_or_else(|| {
                eprintln!("{RED}error{NC}: output path must be provided, use \"-o path\" to pass.");
                exit(1);
            });

            if let Err(e) = apply(&input[0], &input[1], Some(&output), recursive) {
                eprintln!("{RED}error{NC}: {}", e);
                exit(1);
            }
        }
        _ => unreachable!(),
    }
}

fn gettext(input: &str, output: &str, recursive: bool) -> io::Result<()> {
    let input = Path::new(input);
    let mut output_path = PathBuf::from(output);

    let is_dir = output.ends_with("/");
    if input.is_file()
        && let Some(file_stem) = input.file_stem()
        && (is_dir || output_path.is_dir())
    {
        output_path = output_path.join(file_stem).with_extension("pot");
    }
    if input.is_dir() {
        if !recursive {
            println!(
                "{RED}error{NC}: \"-r\" not specified; omitting directory '{}'",
                input.display()
            );
            exit(1);
        } else if !(is_dir || output_path.is_dir()) {
            println!(
                "{RED}error{NC}: \"-r\" specified; but output is not directory '{}'",
                output
            );
            exit(1);
        }
    }

    if recursive
        && let Err(e) = fs::create_dir(output)
        && e.kind() != io::ErrorKind::AlreadyExists
    {
        return Err(e);
    }

    if !recursive {
        let count = do_gettext(input, &output_path)?;
        println!(
            "{GREEN}Success{NC}: extracted {} segments into {CYAN}{}{NC}",
            count,
            output_path.display()
        );
    } else {
        let count = do_gettext_recursively(input, &output_path)?;
        println!(
            "{GREEN}Success{NC}: extracted {} files into {CYAN}{}{NC}",
            count,
            output_path.display()
        );
    }

    Ok(())
}

fn do_gettext(input: &Path, output: &Path) -> io::Result<usize> {
    let twee = fs::read_to_string(input)?;
    let segments = Scanner::new(&twee).run();

    let mut pot = fs::File::create(output)?;
    writeln!(pot, "msgid \"\"")?;
    writeln!(pot, "msgstr \"\"")?;
    writeln!(pot, "\"Project-Id-Version: PACKAGE VERSION\\n\"")?;
    writeln!(pot, "\"Last-Translator: Automatically generated\\n\"")?;
    writeln!(pot, "\"Language-Team: none\\n\"")?;
    writeln!(pot, "\"MIME-Version: 1.0\\n\"")?;
    writeln!(pot, "\"Content-Type: text/plain; charset=UTF-8\\n\"")?;
    writeln!(pot, "\"Content-Transfer-Encoding: 8bit\\n\"")?;

    for seg in segments.iter() {
        writeln!(pot)?;
        if let Some(target) = &seg.link_target {
            writeln!(pot, "#. Link target: {}", target)?;
        }
        writeln!(pot, "#: {}:{}", input.display(), seg.line)?;
        writeln!(pot, "msgctxt \"{}\"", po_escape(&seg.context))?;
        writeln!(pot, "msgid \"{}\"", po_escape(&seg.source))?;
        writeln!(pot, "msgstr \"\"")?;
    }

    pot.flush()?;
    Ok(segments.len())
}

fn do_gettext_recursively(input: &Path, output: &Path) -> io::Result<usize> {
    debug_assert!(input.is_dir());
    debug_assert!(output.is_dir(), "{} is not directory", output.display());

    let mut count = 0;

    for entry in fs::read_dir(input)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        let out = output.join(name);

        if path.is_dir() {
            fs::create_dir_all(&out)?;
            count += do_gettext_recursively(&path, &out)?;
        } else if path.is_file() && path.extension().is_some_and(|ext| ext == "twee") {
            do_gettext(&path, &out.with_extension("pot"))?;
            count += 1;
        }
    }

    Ok(count)
}

fn apply(source: &str, po: &str, output: Option<&str>, recursive: bool) -> io::Result<()> {
    let source = Path::new(source);
    let po = Path::new(po);

    if source.is_dir() || po.is_dir() {
        if !recursive {
            println!(
                "{RED}error{NC}: \"-r\" not specified; omitting directory '{}'",
                if source.is_dir() {
                    source.display()
                } else {
                    po.display()
                }
            );
            exit(1);
        } else if !source.is_dir() {
            println!(
                "{RED}error{NC}: \"-r\" specified; but '{}' is not directory",
                source.display()
            );
            exit(1);
        } else if !po.is_dir() {
            println!(
                "{RED}error{NC}: \"-r\" specified; but '{}' is not directory",
                po.display()
            );
            exit(1);
        }
    }

    let out = match output {
        None => None,
        Some(out) => {
            let mut path = PathBuf::from(out);

            if source.is_file()
                && let Some(file_stem) = source.file_stem()
                && (out.ends_with("/") || path.is_dir())
            {
                path = path.join(file_stem).with_extension("twee");
            }

            if recursive {
                fs::create_dir_all(out)?;
            }

            Some(path)
        }
    };

    if !recursive {
        let result = do_apply(source, po, out.as_deref())?;
        if let Some(o) = out {
            println!(
                "{GREEN}Success{NC}: generated {} from {CYAN}{}{NC} to {CYAN}{}{NC}, {} applied, {} fuzzy, {} empty, {} missing, {} errors",
                result.applied,
                po.display(),
                o.display(),
                result.applied,
                result.fuzzy_skipped,
                result.empty_skipped,
                result.missing,
                result.errors
            );
        } else {
            println!(
                "{CYAN}check{NC}: {} applied, {} fuzzy, {} empty, {} missing, {} errors",
                result.applied,
                result.fuzzy_skipped,
                result.empty_skipped,
                result.missing,
                result.errors
            );
        }
    } else {
        let (count, missing) = do_apply_recursively(source, po, out.as_deref())?;
        if let Some(o) = out {
            println!(
                "{GREEN}Success{NC}: generated {} files into {CYAN}{}{NC}, missing {} po files",
                count,
                o.display(),
                missing
            );
        } else {
            println!(
                "{CYAN}check{NC}: {} files, missing {} po files",
                count, missing
            );
        }
    }

    Ok(())
}

struct ApplyCount {
    applied: usize,
    fuzzy_skipped: usize,
    empty_skipped: usize,
    missing: usize,
    errors: usize,
}

fn do_apply(source_path: &Path, po_path: &Path, output: Option<&Path>) -> io::Result<ApplyCount> {
    let source = fs::read_to_string(source_path)?;
    let po_text = fs::read_to_string(po_path)?;
    let catalog = parse_po(&po_text);

    let segments = Scanner::new(&source).run();
    let result = plan_apply(&source, &segments, &catalog, output.is_none());

    for d in &result.diagnostics {
        let msg = format!(
            "{}\n  {BLUE}-->{NC} {}:{} [{}]",
            d.message,
            source_path.display(),
            d.line,
            d.context
        );
        if d.error {
            eprintln!("{RED}error{NC}: {}", msg);
        } else {
            eprintln!("{YELLOW}warning{NC}: {}", msg);
        }
    }

    if let Some(o) = output {
        fs::write(o, &result.output)?;
    }

    Ok(ApplyCount {
        applied: result.applied,
        fuzzy_skipped: result.fuzzy_skipped,
        empty_skipped: result.empty_skipped,
        missing: result.missing,
        errors: result.errors,
    })
}

fn do_apply_recursively(
    source: &Path,
    po: &Path,
    output: Option<&Path>,
) -> io::Result<(usize, usize)> {
    debug_assert!(source.is_dir(), "{} is not directory", source.display());
    // debug_assert!(po.is_dir(), "{} is not directory", po.display());
    if let Some(o) = output {
        debug_assert!(o.is_dir(), "{} is not directory", o.display());
    }

    let mut count = 0;
    let mut missing = 0;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        let po_path = po.join(&name);
        let out = output.map(|o| o.join(&name));

        if path.is_dir() {
            if !po_path.is_dir() {
                // println!("{YELLOW}warning{NC}: po directory '{}' is not found", po_path.display());
            }

            if let Some(ref o) = out {
                fs::create_dir_all(o)?;
            }
            let (c, miss) = do_apply_recursively(&path, &po_path, out.as_deref())?;
            count += c;
            missing += miss;
        } else if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "twee") {
                let p = po_path.with_extension("po");
                if !p.is_file() {
                    // println!("{YELLOW}warning{NC}: po file '{}' is not found", po_path.display());
                    missing += 1;
                    if let Some(o) = out {
                        fs::copy(&path, &o)?;
                    }
                } else {
                    do_apply(&path, &p, out.map(|o| o.with_extension("twee")).as_deref())?;
                    count += 1;
                }
            } else if let Some(o) = out {
                fs::copy(&path, &o)?;
            }
        }
    }

    Ok((count, missing))
}
