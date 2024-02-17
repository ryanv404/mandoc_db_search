use std::error::Error;
use std::str;

use crate::parse_num;
use crate::pages::Name;

// The MACROS TABLE consists of (in order):
// 1. The total number of MACRO TABLEs (currently 36).
// 2. The index of each MACRO TABLE.
#[derive(Clone, Debug)]
pub struct Macros<'a> {
    pub count: usize,
    pub tables: Vec<Table<'a>>,
}

impl<'a> Macros<'a> {
    pub fn parse(bytes: &'a [u8], start: usize) -> Result<Self, Box<dyn Error>> {
        // Number of macro entries.
        let count = parse_num(bytes, start)?;
        let mut tables = Vec::with_capacity(count);

        let macro_keys_start = start + 4;

        // Iterate over macro entries.
        for i in 0..count {
            let macro_table_idx = parse_num(bytes, macro_keys_start + (i * 4))?;
            let macro_table = Table::parse(bytes, macro_table_idx)?;
            tables.push(macro_table);
        }

        // Ensure the expected number of macros are present.
        if count != 36 || tables.len() != 36 {
            return Err("Macros parsing failed.".into());
        }

        Ok(Self { count, tables })
    }
}

// Each MACRO TABLE consists of (in order):
// 1. The total number of MACRO VALUE entries.
// 2. The MACRO VALUE entries.
#[derive(Clone, Debug)]
pub struct Table<'a> {
    pub count: usize,
    pub values: Vec<Value<'a>>,
}

impl<'a> Table<'a> {
    fn parse(bytes: &'a [u8], start: usize) -> Result<Self, Box<dyn Error>> {
        // Number of macro value entries.
        let count = parse_num(bytes, start)?;
        if count == 0 {
            return Ok(Self { count, values: Vec::new() });
        }

        let values_start = start + 4;
        let mut values = Vec::with_capacity(count);

        // Iterate over macro value entries.
        for i in 0..count {
            let value_idx = values_start + (i * 8);
            let pages_list_idx = value_idx + 4;
            let value = Value::parse(bytes, value_idx, pages_list_idx)?;
            values.push(value);
        }

        // Ensure the expected number of values are present.
        if values.len() != count {
            return Err("Macro values parsing failed.".into());
        }

        Ok(Self { count, values })
    }
}

// Each MACRO VALUE consists of (in order):
// 1. The index of the macro value string (#3 in this table).
// 2. The index of a list of pages (#5 in this table).
// 3. The macro string value.
// 4. Zero to three NUL bytes for padding.
// 5. A list of index values for the list of names for the pages in the list
//    pointed to by #2 of this table.
#[derive(Clone, Debug)]
pub struct Value<'a> {
    pub str: &'a str,
    pub page_names: Vec<Vec<Name<'a>>>,
}

impl<'a> Value<'a> {
    fn parse(
        bytes: &'a [u8],
        value_idx: usize,
        pages_list_idx: usize
    ) -> Result<Self, Box<dyn Error>> {
        let str_idx = parse_num(bytes, value_idx)?;
        let str = bytes[str_idx..]
            .split(|b| *b == 0)
            .next()
            .and_then(|str_bytes| str::from_utf8(str_bytes).ok())
            .ok_or("Macro value parsing failed.")?;

        let mut page_names = Vec::with_capacity(20);
        let pages_list = parse_num(bytes, pages_list_idx)?;

        // Iterate over each page in the pages list.
        for p in 0..=20 {
            let page_idx = parse_num(bytes, pages_list + (p * 4))?;

            // Zero marks the end of the pages list.
            if page_idx == 0 {
                break;
            }

            let names_list = parse_num(bytes, page_idx)?;
            let names_vec = Name::parse_names(bytes, names_list)?;
            page_names.push(names_vec);
        }

        Ok(Self { str, page_names })
    }
}
