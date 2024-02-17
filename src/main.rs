use std::env;
use std::error::Error;
use std::fmt::Debug;
use std::fs;
use std::io::{self, BufRead, Write};
use std::str;

mod macros;
mod pages;
mod utils;

use macros::Macros;
use pages::{PageFormat, Pages};
use utils::{parse_num, print_help, print_list};

const DB_MAGIC_NUMBER: usize = 0x3a7d_0cdb;
const DB_VERSION_NUMBER: usize = 0x1;

fn main() -> Result<(), Box<dyn Error>> {
    let mut do_search = false;
    let args = env::args().collect::<Vec<String>>();

    let db_path = match args.len() {
        2 if args[1] == "-h" || args[1] == "--help" => {
            print_help();
            return Ok(());
        },
        2 if !args[1].starts_with('-') => &args[1],
        3 if (args[1] == "-s" || args[1] == "--search")
            && !args[2].starts_with('-') => {
            do_search = true;
            &args[2]
        },
        _ => {
            print_help();
            return Ok(());
        },
    };

    let bytes = fs::read(db_path)?;
    let db = Database::parse(&bytes)?;

    db.print_summary();

    if !do_search {
        return Ok(());
    }

    println!("* Type \"quit\" to exit.\n");

    let mut out = io::stdout().lock();
    let mut line = String::with_capacity(50);

    loop {
        write!(&mut out, "SEARCH: ")?;
        out.flush()?;

        line.clear();
        io::stdin().lock().read_line(&mut line)?;

        let query = line.trim();
        match query.len() {
            0 => continue,
            1 if query == "q" => break,
            4 if query.eq_ignore_ascii_case("quit") => break,
            _ => db.search(query),
        }
    }

    Ok(())
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
                    page.print();
                    println!();
                    return;
                }
            }
        }

        println!("No results for \"{query}\".\n");
    }

    const fn num_pages(&self) -> usize {
        self.pages.count
    }

    fn num_files(&self) -> usize {
        self.pages.table.iter().map(|p| p.files.len()).sum()
    }

    const fn num_macros(&self) -> usize {
        self.macros.count
    }

    fn print_summary(&self) {
        println!("\
            [MANDOC.DB]\n\
            * Contains {} macro {}.\n\
            * Contains {} man page {} generated from {} man page {}.",
            self.num_macros(),
            if self.num_macros() == 1 { "entry" } else { "entries" },
            self.num_pages(),
            if self.num_pages() == 1 { "entry" } else { "entries" },
            self.num_files(),
            if self.num_files() == 1 { "file" } else { "files" }
        );

        let page_idx_vec = self.pages
            .table
            .iter()
            .enumerate()
            .filter_map(|(idx, page)| match page.format {
                PageFormat::MdocMan => None,
                PageFormat::Preformatted => Some(idx),
            })
            .collect::<Vec<usize>>();

        if page_idx_vec.is_empty() {
            println!("* All pages use man(7) or mdoc(7).");
            return;
        } else if page_idx_vec.len() == 1 {
            print!("* One page does not use man(7) or mdoc(7): ");
        } else {
            let num = page_idx_vec.len();
            print!("* {num} pages do not use man(7) or mdoc(7): ");
        }

        let names = page_idx_vec
            .into_iter()
            .flat_map(|idx| {
                self.pages.table[idx].names.iter().map(|n| n.value)
            })
            .collect::<Vec<&str>>();

        print_list(&names[..]);
    }
}
