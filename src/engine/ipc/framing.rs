use std::io::{BufRead, Write};

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub fn write_msg<W: Write, M: Serialize>(w: &mut W, msg: &M) -> Result<()> {
    serde_json::to_writer(&mut *w, msg)?;
    w.write_all(b"\n")?;
    w.flush()?;
    Ok(())
}

pub fn read_msg<R: BufRead, M: DeserializeOwned>(r: &mut R) -> Result<Option<M>> {
    let mut buf = String::new();
    let n = r.read_line(&mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    let msg = serde_json::from_str(&buf)?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::io::{BufReader, Cursor};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestMsg {
        kind: String,
        n: u32,
    }

    #[test]
    fn write_msg_appends_newline() {
        let mut buf: Vec<u8> = Vec::new();
        let msg = TestMsg {
            kind: "hello".into(),
            n: 1,
        };
        write_msg(&mut buf, &msg).unwrap();

        assert_eq!(buf.last().copied(), Some(b'\n'));
        assert_eq!(buf.iter().filter(|b| **b == b'\n').count(), 1);
    }

    #[test]
    fn read_msg_returns_one_per_line() {
        let mut buf: Vec<u8> = Vec::new();
        let a = TestMsg {
            kind: "a".into(),
            n: 1,
        };
        let b = TestMsg {
            kind: "b".into(),
            n: 2,
        };
        write_msg(&mut buf, &a).unwrap();
        write_msg(&mut buf, &b).unwrap();

        let mut r = BufReader::new(Cursor::new(buf));
        let got_a: Option<TestMsg> = read_msg(&mut r).unwrap();
        let got_b: Option<TestMsg> = read_msg(&mut r).unwrap();
        let got_eof: Option<TestMsg> = read_msg(&mut r).unwrap();

        assert_eq!(got_a, Some(a));
        assert_eq!(got_b, Some(b));
        assert_eq!(got_eof, None);
    }

    #[test]
    fn read_msg_eof_returns_none() {
        let mut r = BufReader::new(Cursor::new(Vec::<u8>::new()));
        let got: Option<TestMsg> = read_msg(&mut r).unwrap();
        assert_eq!(got, None);
    }

    #[test]
    fn read_msg_malformed_json_returns_err() {
        let mut r = BufReader::new(Cursor::new(b"not json\n".to_vec()));
        let res: Result<Option<TestMsg>> = read_msg(&mut r);
        assert!(res.is_err());
    }
}
