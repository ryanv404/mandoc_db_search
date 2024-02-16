use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::error::Error;
use std::str;

use crate::{parse_num, parse_strings_list};

#[derive(Clone)]
pub struct Name<'a> {
    pub str: &'a str,
    pub source: u8,
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

        Ok(Self { names, sects, archs, desc, files, format })
    }
}
