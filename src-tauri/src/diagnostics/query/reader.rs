use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

pub(super) const READ_BLOCK_BYTES: usize = 64 * 1024;
pub(super) const MAX_LINE_BYTES: usize = 64 * 1024;
pub(super) const MAX_SCAN_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReadMetrics {
    pub scanned_bytes: u64,
    pub damaged_lines: u64,
    pub oversized_lines: u64,
    pub incomplete_tail_lines: u64,
}

pub(super) fn read_lines_reverse<F>(
    path: &Path,
    upper_bound: u64,
    metrics: &mut ReadMetrics,
    mut visit: F,
) -> io::Result<bool>
where
    F: FnMut(&[u8]) -> bool,
{
    let mut file = File::open(path)?;
    let actual_len = file.metadata()?.len();
    if actual_len < upper_bound {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "operation history snapshot was truncated",
        ));
    }

    let mut position = upper_bound;
    let mut line_reversed = Vec::with_capacity(1024);
    let mut line_oversized = false;
    let mut skipping_incomplete_tail = if upper_bound == 0 {
        false
    } else {
        file.seek(SeekFrom::Start(upper_bound - 1))?;
        let mut byte = [0u8; 1];
        file.read_exact(&mut byte)?;
        byte[0] != b'\n'
    };
    if skipping_incomplete_tail {
        metrics.incomplete_tail_lines = metrics.incomplete_tail_lines.saturating_add(1);
    }

    while position > 0 {
        if metrics.scanned_bytes >= MAX_SCAN_BYTES {
            return Ok(false);
        }
        let remaining_budget = MAX_SCAN_BYTES.saturating_sub(metrics.scanned_bytes);
        let block_len = position.min(READ_BLOCK_BYTES as u64).min(remaining_budget);
        if block_len == 0 {
            return Ok(false);
        }
        let start = position - block_len;
        let block_len = usize::try_from(block_len).map_err(io::Error::other)?;
        let mut block = vec![0u8; block_len];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut block)?;
        metrics.scanned_bytes = metrics
            .scanned_bytes
            .saturating_add(u64::try_from(block_len).unwrap_or(u64::MAX));
        position = start;

        for byte in block.into_iter().rev() {
            if skipping_incomplete_tail {
                if byte == b'\n' {
                    skipping_incomplete_tail = false;
                }
                continue;
            }
            if byte == b'\n' {
                if line_oversized {
                    metrics.oversized_lines = metrics.oversized_lines.saturating_add(1);
                } else if !line_reversed.is_empty() {
                    line_reversed.reverse();
                    if !visit(&line_reversed) {
                        return Ok(true);
                    }
                }
                line_reversed.clear();
                line_oversized = false;
            } else if !line_oversized {
                if line_reversed.len() == MAX_LINE_BYTES {
                    line_reversed.clear();
                    line_oversized = true;
                } else {
                    line_reversed.push(byte);
                }
            }
        }
    }

    if line_oversized {
        metrics.oversized_lines = metrics.oversized_lines.saturating_add(1);
    } else if !line_reversed.is_empty() {
        line_reversed.reverse();
        let _ = visit(&line_reversed);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn reconstructs_a_line_that_crosses_a_block_boundary() {
        let directory = tempfile::tempdir().expect("temporary directory created");
        let path = directory.path().join("fixture.log");
        let long_line = "x".repeat(READ_BLOCK_BYTES - 13);
        let contents = format!("first\n{long_line}\nlast\n");
        fs::write(&path, contents.as_bytes()).expect("fixture writes");
        let mut metrics = ReadMetrics::default();
        let mut lines = Vec::new();

        read_lines_reverse(&path, contents.len() as u64, &mut metrics, |line| {
            lines.push(String::from_utf8(line.to_vec()).expect("line is utf-8"));
            true
        })
        .expect("fixture reads");

        assert_eq!(lines, vec!["last", long_line.as_str(), "first"]);
        assert_eq!(metrics.scanned_bytes, contents.len() as u64);
        assert_eq!(metrics.oversized_lines, 0);
    }
}
