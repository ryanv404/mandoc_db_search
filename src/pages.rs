use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::str;

use crate::utils::{parse_list, parse_num, print_list};

// The Pages table consists of (in order):
// 1. The total number of Page entries.
// 2. The Page entries.
#[derive(Clone, Debug)]
pub struct Pages<'a> {
    pub count: usize,
    pub table: Vec<Page<'a>>,
}

impl<'a> Pages<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Box<dyn Error>> {
        // The total number of pages is at offset 16.
        let count = parse_num(bytes, 16)?;
        let mut table = Vec::with_capacity(count);

        // The page entries begin at offset 20.
        let table_idx = 20;

        // Each page entry is 20 bytes.
        let page_size = 20;

        for page_idx in 0..count {
            let offset = page_size * page_idx;
            let page = Page::parse(bytes, table_idx + offset)?;
            table.push(page);
        }

        // Ensure the expected number of pages are present.
        if table.len() != count {
            return Err("Page entry parsing failed.".into());
        }

        Ok(Self { count, table })
    }
}

#[derive(Clone)]
pub struct Name<'a> {
    pub value: &'a str,
    pub source: u8,
}

impl<'a> Display for Name<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.value)
    }
}

impl<'a> Debug for Name<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{:?}", self.value)
    }
}

impl<'a> Name<'a> {
    pub fn parse_names(
        bytes: &'a [u8],
        start: usize
    ) -> Result<Vec<Name<'a>>, Box<dyn Error>> {
        let mut names = Vec::with_capacity(10);
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
                    let (src, name_bytes) = item_bytes[..(len - 1)]
                        .split_first()
                        .ok_or("Names list parsing failed.")?;

                    let name = str::from_utf8(name_bytes)?;
                    names.push(Self { value: name, source: *src });
                },
            }
        }

        Ok(names)
    }
}

#[derive(Debug, Clone)]
pub enum PageFormat {
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
pub struct Page<'a> {
    pub names: Vec<Name<'a>>,
    pub sects: Vec<&'a str>,
    pub archs: Option<Vec<&'a str>>,
    pub desc: &'a str,
    pub files: Vec<&'a str>,
    pub format: PageFormat,
}

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
//
// The bits in a name sources byte indicate where the name appears:
// 0b00000001: a SYNOPSIS section .Nm block.
// 0b00000010: any NAME section .Nm macro.
// 0b00000100: the first NAME section .Nm macro.
// 0b00001000: a header line (i.e. a .Dt or .TH macro).
// 0b00010000: a file name.
impl<'a> Page<'a> {
    pub fn parse(
        bytes: &'a [u8],
        start: usize
    ) -> Result<Self, Box<dyn Error>> {
        assert!(start + 19 < bytes.len());

        let names_start = parse_num(bytes, start)?;
        let sects_start = parse_num(bytes, start + 4)?;
        let archs_start = parse_num(bytes, start + 8)?;
        let desc_start = parse_num(bytes, start + 12)?;
        let files_start = parse_num(bytes, start + 16)?;

        let names = Name::parse_names(bytes, names_start)?;
        let sects = parse_list(bytes, sects_start)?;
        let archs = if archs_start != 0 {
            Some(parse_list(bytes, archs_start)?)
        } else {
            None
        };
        let desc = bytes[desc_start..]
            .split(|b| *b == 0)
            .next()
            .and_then(|desc_bytes| str::from_utf8(desc_bytes).ok())
            .ok_or("Description string parsing failed.")?;
        let files = parse_list(bytes, files_start + 1)?;
        let format = PageFormat::from(bytes[files_start]);

        Ok(Self { names, sects, archs, desc, files, format })
    }

    pub fn print(&self) {
        let names = self.names.iter().map(|n| n.value).collect::<Vec<&str>>();
        print!("* Names: ");
        print_list(&names[..]);
        print!("* Sections: ");
        print_list(&self.sects[..]);
        print!("* Architectures: ");
        self.archs.as_ref().map_or_else(
            || println!("machine-independent"),
            |archs| print_list(&archs[..]));
        println!("* Description: {}", self.desc);
        print!("* Files: ");
        print_list(&self.files[..]);
        println!("* Format: {}", self.format);
    }
}
