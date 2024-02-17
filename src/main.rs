#![deny(clippy::all)]
#![deny(clippy::cargo)]
#![deny(clippy::complexity)]
#![deny(clippy::correctness)]
#![deny(clippy::nursery)]
#![deny(clippy::pedantic)]
#![deny(clippy::perf)]
#![deny(clippy::style)]
#![deny(clippy::suspicious)]

use std::convert::TryFrom;
use std::env;
use std::error::Error;
use std::fmt::Debug;
use std::fs;
use std::io::{self, BufRead, Write};
use std::num::TryFromIntError;
use std::str;

mod macros;
mod pages;

use pages::{PageFormat, Pages};
use macros::Macros;

const DB_MAGIC_NUMBER: usize = 0x3a7d0cdb;
const DB_VERSION_NUMBER: usize = 0x1;

fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().collect::<Vec<String>>();

    if args.len() != 2 {
        let name = env!("CARGO_PKG_NAME");
        eprintln!("usage: ./{name} <MANDOC_DB_FILE_PATH>");
        return Ok(());
    }

    let bytes = fs::read(&args[1])?;
    let db = Database::parse(&bytes)?;

    db.print_intro();

    let mut out = io::stdout().lock();
    let mut line = String::with_capacity(100);

    loop {
        write!(&mut out, "SEARCH: ")?;
        out.flush()?;

        line.clear();
        io::stdin()
            .lock()
            .read_line(&mut line)?;

        let query = line.trim();

        if query.is_empty() {
            continue;
        } else if query.eq_ignore_ascii_case("quit") {
            break;
        } else {
            db.search(query);
        }
    }

    Ok(())
}

fn parse_num(bytes: &[u8], start: usize) -> Result<usize, TryFromIntError> {
    assert!(start + 3 < bytes.len());

    let mut int_bytes = [0u8; 4];
    int_bytes.copy_from_slice(&bytes[start..=start + 3]);
    usize::try_from(u32::from_be_bytes(int_bytes))
}

fn parse_list(
    bytes: &[u8],
    start: usize
) -> Result<Vec<&str>, Box<dyn Error>> {
    let mut list = Vec::with_capacity(10);
    let strings_iter = bytes[start..].split_inclusive(|b| *b == 0);

    for string_bytes in strings_iter {
        match string_bytes.len() {
            0 => return Err("Parsed an unexpected empty string.".into()),
            // A NUL byte marks the end of a list.
            1 if string_bytes[0] == 0 => break,
            len => {
                let s = str::from_utf8(&string_bytes[..(len - 1)])?;
                list.push(s);
            },
        }
    }

    Ok(list)
}

// Database data types:
// * Number: a 32-bit signed integer with big endian byte order.
// * String: a NUL-terminated array of bytes.
// * Strings list: An array of strings that is terminated by a second NUL
//   following the final entry.
//
// A mandoc.db file consists of (in order):
// 1. The "magic number" (i.e. 0x3a7d0cdb).
// 2. The version number (currently 1).
// 3. The index of the MACROS TABLE.
// 4. The index of the "magic number" located at the end of the file.
// 5. The PAGES TABLE.
// 6. The MACROS TABLE.
// 7. The "magic number", again.
#[derive(Debug, Clone)]
pub struct Database<'a> {
    pub pages: Pages<'a>,
    pub macros: Macros<'a>,
}

impl<'a> Database<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        let first_four = parse_num(bytes, 0)?;
        let second_four = parse_num(bytes, 4)?;
        let final_four_idx = parse_num(bytes, 12)?;
        let final_four = parse_num(bytes, final_four_idx)?;

        // The first 4 bytes and last 4 bytes should be the magic number.
        if first_four != DB_MAGIC_NUMBER || final_four != DB_MAGIC_NUMBER {
            return Err("Invalid file format.".into());
        }

        // The second 4 bytes should be the version number.
        if second_four != DB_VERSION_NUMBER {
            return Err("Invalid version number.".into());
        }

        let pages = Pages::parse(bytes)?;
        let macros_idx = parse_num(bytes, 8)?;
        let macros = Macros::parse(bytes, macros_idx)?;

        Ok(Self { pages, macros })
    }

    fn search(&self, query: &str) {
        for page in &self.pages.table {
            for name in &page.names {
                if name.value.eq_ignore_ascii_case(query) {
                    println!("{}\n", &page);
                    return;
                }
            }
        }

        println!("No results for \"{query}\".\n");
    }

    fn print_intro(&self) {
        println!(
            "[MANDOC.DB]\n* Contains {} man page {}.",
            self.pages.count,
            if self.pages.count == 1 { "entry" } else { "entries" }
        );

        let unknowns_iter = self.pages.table.iter();
        let unknowns = unknowns_iter
            .enumerate()
            .filter_map(|(idx, page)| match page.format {
                PageFormat::MdocMan => None,
                PageFormat::Preformatted => Some(idx),
            })
            .collect::<Vec<usize>>();

        match unknowns.len() {
            0 => {
                println!("* All pages use man(7) or mdoc(7).\n");
                return;
            },
            1 => println!("* One page does not use man(7) or mdoc(7)."),
            num => println!("* {num} pages do not use man(7) or mdoc(7)."),
        }

        for (count, idx) in unknowns.iter().enumerate() {
            if count == 5 {
                // Only print the first 5 items.
                println!("    - ...\n");
                return;
            } else if self.pages.table[*idx].names.len() == 1 {
                println!("    - {}", self.pages.table[*idx].names[0]);
            } else {
                println!("    - {:?}", &self.pages.table[*idx].names);
            }
        }

        println!("* Type \"quit\" to exit.\n");
    }
}
