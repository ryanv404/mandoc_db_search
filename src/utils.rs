use std::num::TryFromIntError;
use std::str;

pub fn print_list(list: &[&str]) {
    if list.is_empty() {
        println!();
        return;
    }

    let last_idx = list.len() - 1;

    for (count, item) in list.iter().enumerate() {
        if count == last_idx {
            println!("{item}");
            return;
        }

        print!("{item}, ");
    }
}

pub fn print_help() {
    let name = env!("CARGO_PKG_NAME");
    println!("USAGE:\n  ./{name} [OPTIONS] <MANDOC_DB_FILE_PATH>\n");
    println!("OPTIONS:");
    println!("  -h,--help     Print this help message.");
    println!("  -s,--search   Search for a page entry by name.");
}

pub fn parse_num(bytes: &[u8], idx: usize) -> Result<usize, TryFromIntError> {
    assert!(idx + 3 < bytes.len());
    let mut int_bytes = [0u8; 4];
    int_bytes.copy_from_slice(&bytes[idx..=idx + 3]);
    usize::try_from(u32::from_be_bytes(int_bytes))
}

pub fn parse_list(
    bytes: &[u8],
    idx: usize
) -> Result<Vec<&str>, &'static str> {
    let mut list = Vec::with_capacity(20);
    let split_iter = bytes[idx..].split_inclusive(|b| *b == 0);

    for item_bytes in split_iter {
        match item_bytes.len() {
            0 => return Err("Encountered an unexpected NUL byte."),
            // A NUL byte marks the end of a list.
            1 if item_bytes[0] == 0 => break,
            len => {
                let item_str = str::from_utf8(&item_bytes[..(len - 1)])
                    .map_err(|_| "str::from_utf8 failed while parsing a list.")?;
                list.push(item_str);
            },
        }
    }

    Ok(list)
}
