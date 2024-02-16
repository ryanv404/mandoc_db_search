use std::convert::TryFrom;
use std::env;
use std::error::Error;
use std::fmt::Debug;
use std::fs;
use std::io::{self, BufRead, Write};
use std::num::TryFromIntError;
use std::str;

mod page;
use page::{Page, PageFormat};

// Data types utilized by the database:
// * Number: a 32-bit signed integer with big endian byte order.
// * String: a NUL-terminated array of bytes.
// * Strings list: An array of strings that is terminated by a second NUL
//   following the final entry.

// A mandoc.db file consists of (in order):
// 1. The "magic number" (i.e. 0x3a7d0cdb).
// 2. The version number (currently 1).
// 3. The index of the MACROS TABLE.
// 4. The index of the "magic number" located at the end of the file.
// 5. The PAGES TABLE.
// 6. The MACROS TABLE.
// 7. The "magic number", again.

// The PAGES TABLE consists of (in order):
// 1. The total number of PAGE entries.
// 2. The PAGE entries.
//
// Each PAGE entry consists of (in order):
// 1. The index of the name strings list.
//   a. Each name consists of (in order):
//     * A name sources byte (see below).
//     * The name string.
// 2. The index of the section strings list.
// 3. The index of the architecture strings list.
//   a. An index value of 0 indicates the page is machine-independent.
// 4. The index of the one-line description string.
// 5. The index of the filename strings list.
//   a. The first filename is preceded a byte indicating the page's format:
//     * 0x01: either mdoc(7) or man(7).
//     * 0x02: preformatted.

// The bits in a name sources byte indicate where the name appears:
// 0b00000001: a SYNOPSIS section .Nm block.
// 0b00000010: any NAME section .Nm macro.
// 0b00000100: the first NAME section .Nm macro.
// 0b00001000: a header line (i.e. a .Dt or .TH macro).
// 0b00010000: a file name.

// The MACROS TABLE consists of (in order):
// 1. The total number of MACRO TABLEs (currently 36).
// 2. The index of each MACRO TABLE.
//
// Each MACRO TABLE consists of (in order):
// 1. The total number of MACRO VALUE entries.
// 2. The MACRO VALUE entries.
//
// Each MACRO VALUE consists of (in order):
// 1. The index of the macro value string (#3 in this table).
// 2. The index of a list of pages (#5 in this table).
// 3. The macro string value.
// 4. Zero to three NUL bytes for padding.
// 5. A list of index values for the list of names for the pages in the list
//    pointed to by #2 of this table.

const DB_MAGIC_NUMBER: usize = 0x3a7d0cdb;
const DB_VERSION_NUMBER: usize = 0x1;

fn main() -> Result<(), Box<dyn Error>> {
    let args = env::args().collect::<Vec<String>>();

    if args.len() < 2 {
        return Err("Missing mandoc.db file path argument.".into());
    }

    let bytes = fs::read(&args[1])?;

    let db = Database::parse(&bytes)?;
    db.print_intro();

    let mut out = io::stdout().lock();
    let mut line = String::with_capacity(250);

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

fn parse_strings_list(
    bytes: &[u8],
    start: usize
) -> Result<Vec<&str>, Box<dyn Error>> {
    let mut strings_list = Vec::with_capacity(10);
    let strings_iter = bytes[start..].split_inclusive(|b| *b == 0);

    for string_bytes in strings_iter {
        match string_bytes.len() {
            0 => return Err("Parsed an unexpected empty string.".into()),
            // A NUL byte marks the end of a list.
            1 if string_bytes[0] == 0 => break,
            len => {
                let s = str::from_utf8(&string_bytes[..(len - 1)])?;
                strings_list.push(s);
            },
        }
    }

    Ok(strings_list)
}

#[derive(Debug, Clone)]
struct Database<'a> {
    total_pages: usize,
    pages: Vec<Page<'a>>,
}

impl<'a> Database<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        // The first 4 bytes and last 4 bytes should be the magic number.
        let first_four = parse_num(bytes, 0)?;
        let final_four_idx = parse_num(bytes, 12)?;
        let final_four = parse_num(bytes, final_four_idx)?;
        if first_four != DB_MAGIC_NUMBER || final_four != DB_MAGIC_NUMBER {
            return Err("Invalid file format.".into());
        }

        // The second 4 bytes should be the version number.
        let second_four = parse_num(bytes, 4)?;
        if second_four != DB_VERSION_NUMBER {
            return Err("Invalid version number.".into());
        }

        let page_size = 20;
        let pages_start_idx = 20;
        let total_pages = parse_num(bytes, 16)?;
        let mut pages = Vec::with_capacity(total_pages);

        for page_idx in 0..total_pages {
            // Pages table starts at 20 bytes and a page's size is 20 bytes.
            let start_idx = pages_start_idx + (page_size * page_idx);
            pages.push(Page::parse(bytes, start_idx)?);
        }

        // Ensure the expected number of pages are present.
        if pages.len() != total_pages {
            return Err("Page entry parsing failed.".into());
        }

        Ok(Self { total_pages, pages })
    }

    fn search(&self, query: &str) {
        for page in &self.pages {
            for name in &page.names {
                if name.str.eq_ignore_ascii_case(query) {
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
            self.total_pages,
            if self.total_pages == 1 { "entry" } else { "entries" }
        );

        let unknowns = self
            .pages
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.format {
                PageFormat::MdocMan => None,
                PageFormat::Preformatted => Some(i),
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

        for (count, page_idx) in unknowns.iter().enumerate() {
            if count == 5 {
                // Only print the first 5 items.
                println!("    - ...\n");
                return;
            } else if self.pages[*page_idx].names.len() == 1 {
                println!("    - {}", self.pages[*page_idx].names[0]);
            } else {
                println!("    - {:?}", &self.pages[*page_idx].names);
            }
        }

        println!("* Type \"quit\" to exit.\n");
    }
}
