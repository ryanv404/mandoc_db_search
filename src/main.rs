use std::convert::TryFrom;
use std::env;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::fs;
use std::io::{self, BufRead, Write};
use std::num::TryFromIntError;
use std::str;

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

    db.print_summary();

    let mut out = io::stdout().lock();
    let mut line = String::with_capacity(250);

    loop {
        write!(&mut out, "SEARCH: ")?;
        out.flush()?;

        line.clear();
        io::stdin().lock().read_line(&mut line)?;
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

    fn print_summary(&self) {
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

        println!();
    }
}

#[derive(Clone)]
struct Name<'a> {
    str: &'a str,
    source: u8,
}

impl<'a> Display for Name<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.str)
    }
}

impl<'a> Debug for Name<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:?}", self.str)
    }
}

impl<'a> Name<'a> {
    fn parse_names_list(
        bytes: &'a [u8],
        start: usize
    ) -> Result<Vec<Name<'a>>, Box<dyn Error>> {
        let mut names_list = Vec::with_capacity(10);
        let item_iter = bytes[start..].split_inclusive(|b| *b == 0);

        for item_bytes in item_iter {
            match item_bytes.len() {
                0 => return Err("Parsed an unexpected empty string.".into()),
                // A NUL byte marks the end of a list.
                1 if item_bytes[0] == 0 => break,
                _ if !matches!(item_bytes[0], 1..=31) => {
                    return Err("Name source parsing failed.".into());
                },
                len => {
                    // We know the slice is not empty so it is safe to unwrap.
                    let (name_src, name_bytes) = item_bytes[..(len - 1)]
                        .split_first()
                        .ok_or("Names list parsing failed.")?;

                    let name_str = str::from_utf8(name_bytes)?;
                    names_list.push(Self { str: name_str, source: *name_src });
                },
            }
        }

        Ok(names_list)
    }

//    fn print_sources(&self) {
//        // 0x01: Name appears in the SYNOPSIS section.
//        // 0x02: Name appears in the NAME section.
//        // 0x04: Name is the first one in the NAME section.
//        // 0x08: Name appears in a header line.
//        // 0x10: Name appears in the file name.
//    }
}

#[derive(Debug, Clone)]
enum PageFormat {
    // 0x01: The file format is mdoc(7) or man(7).
    MdocMan,
    // 0x02: The manual page is preformatted.
    Preformatted,
}

impl Display for PageFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::MdocMan => f.write_str("man(7) or mdoc(7)"),
            Self::Preformatted => f.write_str("preformatted"),
        }
    }
}

impl From<u8> for PageFormat {
    fn from(byte: u8) -> Self {
        match byte {
            1 => Self::MdocMan,
            2 => Self::Preformatted,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
struct Page<'a> {
    names: Vec<Name<'a>>,
    sects: Vec<&'a str>,
    archs: Option<Vec<&'a str>>,
    desc: &'a str,
    files: Vec<&'a str>,
    format: PageFormat,
}

impl<'a> Display for Page<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        for (i, name) in self.names.iter().enumerate() {
            writeln!(f, "* Name[{i}]: {name} (source: 0x{:02x})", name.source)?;
        }

        for (i, sect) in self.sects.iter().enumerate() {
            writeln!(f, "* Section[{i}]: {sect}")?;
        }

        match self.archs.as_ref() {
            None => writeln!(f, "* Arch: machine-independent")?,
            Some(archs) => {
                for (i, arch) in archs.iter().enumerate() {
                    writeln!(f, "* Arch[{i}]: {arch}")?;
                }
            },
        }

        writeln!(f, "* Desc: {}", &self.desc)?;

        for (i, file) in self.files.iter().enumerate() {
            writeln!(f, "* File[{i}]: {file}")?;
        }

        write!(f, "* Format: {}", self.format)
    }
}

impl<'a> Page<'a> {
    fn parse(bytes: &'a [u8], start: usize) -> Result<Self, Box<dyn Error>> {
        assert!(start + 19 < bytes.len());

        let names_start = parse_num(bytes, start)?;
        let sects_start = parse_num(bytes, start + 4)?;
        let archs_start = parse_num(bytes, start + 8)?;
        let desc_start = parse_num(bytes, start + 12)?;
        let files_start = parse_num(bytes, start + 16)?;

        let names = Name::parse_names_list(bytes, names_start)?;
        let sects = parse_strings_list(bytes, sects_start)?;
        let archs = if archs_start != 0 {
            Some(parse_strings_list(bytes, archs_start)?)
        } else {
            None
        };
        let desc = bytes[desc_start..]
            .split(|b| *b == 0)
            .next()
            .and_then(|desc_bytes| str::from_utf8(desc_bytes).ok())
            .ok_or("Description string parsing failed.")?;
        let files = parse_strings_list(bytes, files_start + 1)?;
        let format = PageFormat::from(bytes[files_start]);

        Ok(Self {
            names,
            sects,
            archs,
            desc,
            files,
            format
        })
    }
}
