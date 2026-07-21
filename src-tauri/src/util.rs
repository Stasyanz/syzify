use std::io::Read;

/// Read at most `max` bytes from `reader`, erroring if it would exceed that.
///
/// Reading through `take(max + 1)` bounds memory even when a (zip) entry claims a
/// huge uncompressed size, so a crafted archive can't OOM the app. `what` names
/// the source for the error message.
pub fn read_capped(reader: &mut impl Read, max: u64, what: &str) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    reader
        .take(max + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("failed reading {what}: {e}"))?;
    if buf.len() as u64 > max {
        return Err(format!("{what} exceeds the {max}-byte limit"));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_within_cap_and_rejects_over() {
        let data = [7u8; 100];
        assert_eq!(read_capped(&mut &data[..], 100, "x").unwrap().len(), 100);
        assert_eq!(read_capped(&mut &data[..], 200, "x").unwrap().len(), 100);
        assert!(read_capped(&mut &data[..], 99, "x").is_err());
    }
}
