pub fn find_fenced_code_ranges(content: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut pos = 0;
    let bytes = content.as_bytes();
    while pos < bytes.len() {
        if bytes[pos] == b'`' && pos + 2 < bytes.len() && bytes[pos + 1] == b'`' && bytes[pos + 2] == b'`' {
            let fence_start = pos;
            pos += 3;
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            let mut found_close = false;
            while pos < bytes.len() {
                if bytes[pos] == b'\n' && pos + 3 < bytes.len()
                    && bytes[pos + 1] == b'`' && bytes[pos + 2] == b'`' && bytes[pos + 3] == b'`'
                {
                    pos += 4;
                    while pos < bytes.len() && bytes[pos] != b'\n' {
                        pos += 1;
                    }
                    ranges.push((fence_start, pos));
                    found_close = true;
                    break;
                }
                pos += 1;
            }
            if !found_close {
                ranges.push((fence_start, bytes.len()));
            }
        } else {
            pos += 1;
        }
    }
    ranges
}

pub fn is_inside_fence(ranges: &[(usize, usize)], offset: usize) -> bool {
    ranges.iter().any(|&(start, end)| offset >= start && offset < end)
}
