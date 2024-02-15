use std::convert::TryFrom;
use std::env;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;
use std::str;

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<String>>();

    if args.len() < 2 {
        eprintln!("Missing mandoc.db file path.");
        return ExitCode::from(1);
    }

    let bytes = fs::read(&args[1]).expect("Cannot open file");
    let db = Database::parse(&bytes);

    let other_formats = db
        .pages
        .iter()
        .enumerate()
        .filter(|(_, pg)| !pg.is_mdoc_or_man)
        .map(|(i, _)| i)
        .collect::<Vec<usize>>();

    println!("[MANDOC.DB]: Contains {} man page entries.", &db.pages.len());
    if other_formats.is_empty() {
        println!("[MANDOC.DB]: All pages use man(7)/mdoc(7) formatting.\n");
    } else {
        println!(
            "[MANDOC.DB]: {} pages do not use man(7)/mdoc(7) formatting.",
            other_formats.len()
        );
        for (i, page_idx) in other_formats.iter().enumerate() {
            if i > 5 {
                // Only print the first 5 items.
                println!("             - ...");
                break;
            }

            let names = &db.pages[*page_idx].names;
            if names.len() == 1 {
                println!("             - {}", &names[0]);
            } else {
                println!("             - {:?}", names);
            }
        }
        println!();
    }

    let mut out = io::stdout().lock();
    let mut line = String::with_capacity(250);

    loop {
        line.clear();
        write!(&mut out, "Search: ").unwrap();
        out.flush().unwrap();
        io::stdin().lock().read_line(&mut line).unwrap();

        match line.trim() {
            query if query.is_empty() => continue,
            query if query.eq_ignore_ascii_case("quit") => break,
            query => db.search(query),
        }
    }

    ExitCode::SUCCESS
}

fn parse_num(bytes: &[u8], start: usize) -> usize {
    assert!(start + 3 < bytes.len());

    let mut int_bytes = [0u8; 4];
    int_bytes.copy_from_slice(&bytes[start..=start + 3]);
    usize::try_from(u32::from_be_bytes(int_bytes)).expect("usize conversion")
}

fn parse_string(bytes: &[u8], start: usize) -> Option<&str> {
    bytes[start..]
        .split(|b| *b == 0)
        .next()
        .and_then(|str_bytes| str::from_utf8(str_bytes).ok())
}

fn parse_strings_list(bytes: &[u8], start: usize) -> Vec<&str> {
    let mut str_list = Vec::new();
    let str_iter = bytes[start..].split_inclusive(|b| *b == 0);

    for str_bytes in str_iter {
        assert!(!str_bytes.is_empty());

        if str_bytes == [0] {
            // Two successive NUL bytes mark the end of a list.
            break;
        }

        let str = parse_string(str_bytes, 0).expect("string parsing");
        str_list.push(str);
    }

    str_list
}

#[derive(Debug, Clone)]
struct Database<'a> {
    _magic_num: usize,
    _version_num: usize,
    _total_pages: usize,
    pages: Vec<Page<'a>>,
}

impl<'a> Database<'a> {
    fn parse(bytes: &'a [u8]) -> Self {
        let _magic_num = parse_num(bytes, 0);
        let _version_num = parse_num(bytes, 4);
        let _mtable_start = parse_num(bytes, 8);
        let mtable_end = parse_num(bytes, 12) - 1;
        let end_of_db = parse_num(bytes, mtable_end + 1);

        assert_eq!(end_of_db, _magic_num);

        let _total_pages = parse_num(bytes, 16);
        let mut pages = Vec::with_capacity(_total_pages);

        for page_num in 0.._total_pages {
            // Pages table starts at 20 bytes and a page's size is 20 bytes.
            let start = 20 + (20 * page_num);
            pages.push(Page::parse(bytes, start));
        }

        Self {
            _magic_num,
            _version_num,
            _total_pages,
            pages
        }
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
}

#[derive(Clone)]
struct Name<'a> {
    str: &'a str,
    _source: u8,
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
    fn parse_strings_list(bytes: &'a [u8], start: usize) -> Vec<Name<'a>> {
        let mut str_list = Vec::new();
        let str_iter = bytes[start..].split_inclusive(|b| *b == 0);

        for str_bytes in str_iter {
            assert!(!str_bytes.is_empty());

            if str_bytes == [0] {
                // Two successive NUL bytes mark the end of a list.
                break;
            }

            assert!(matches!(str_bytes[0], 1..=31));

            let (name_src, name_bytes) = str_bytes.split_first().unwrap();
            let str = parse_string(name_bytes, 0).expect("name string parsing");
            str_list.push(Self { str, _source: *name_src });
        }

        str_list
    }
}

#[derive(Debug, Clone)]
struct Page<'a> {
    names: Vec<Name<'a>>,
    sections: Vec<&'a str>,
    archs: Option<Vec<&'a str>>,
    description: &'a str,
    files: Vec<&'a str>,
    is_mdoc_or_man: bool,
}

impl<'a> Display for Page<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        if self.names.len() == 1 {
            writeln!(f, "--name: {}", self.names[0])?;
        } else {
            writeln!(f, "--names: {:?}", &self.names)?;
        }
        if self.sections.len() == 1 {
            writeln!(f, "--section: {}", &self.sections[0])?;
        } else {
            writeln!(f, "--sections: {:?}", &self.sections)?;
        }
        if let Some(ref archs) = self.archs {
            if archs.len() == 1 {
                writeln!(f, "--architecture: {}", &archs[0])?;
            } else {
                writeln!(f, "--architectures: {:?}", &archs)?;
            }
        } else {
            writeln!(f, "--architecture: machine-independent")?;
        }
        writeln!(f, "--description: {}", &self.description)?;
        if self.files.len() == 1 {
            writeln!(f, "--file: {}", &self.files[0])?;
        } else {
            writeln!(f, "--files: {:?}", &self.files)?;
        }
        write!(f, "--is_mdoc_or_man: {}", self.is_mdoc_or_man)?;
        Ok(())
    }
}

impl<'a> Page<'a> {
    fn parse(bytes: &'a [u8], start: usize) -> Self {
        assert!(start + 19 < bytes.len());

        let names_start = parse_num(bytes, start);
        let names = Name::parse_strings_list(bytes, names_start);

        let sections_start = parse_num(bytes, start + 4);
        let sections = parse_strings_list(bytes, sections_start);

        let archs_start = parse_num(bytes, start + 8);
        let archs = if archs_start != 0 {
            Some(parse_strings_list(bytes, archs_start))
        } else {
            None
        };

        let description_start = parse_num(bytes, start + 12);
        let description = parse_string(bytes, description_start)
            .expect("string parsing");

        let files_start = parse_num(bytes, start + 16);
        let files = parse_strings_list(bytes, files_start + 1);
        let is_mdoc_or_man = bytes[files_start] == 1;

        Self {
            names,
            sections,
            archs,
            description,
            files,
            is_mdoc_or_man
        }
    }
}
