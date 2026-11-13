//! 字面字节替换，支持跨块匹配；调用方负责提供要隐藏的内容。
use zeroize::Zeroizing;

#[derive(Clone, Default)]
pub(crate) struct TextRedactor {
    patterns: Vec<Zeroizing<Vec<u8>>>,
    pending: Zeroizing<Vec<u8>>,
    keep: usize,
}

impl TextRedactor {
    pub(crate) fn extended<'a>(&self, secrets: impl IntoIterator<Item = &'a str>) -> Self {
        let mut patterns = self.patterns.clone();
        for secret in secrets {
            if secret.is_empty() {
                continue;
            }
            patterns.push(Zeroizing::new(secret.as_bytes().to_vec()));
            let escaped = serde_json::to_string(secret).expect("string serializes");
            patterns.push(Zeroizing::new(
                escaped.as_bytes()[1..escaped.len() - 1].to_vec(),
            ));
        }
        patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.len()));
        patterns.dedup();
        let keep = patterns
            .iter()
            .map(|pattern| pattern.len())
            .max()
            .unwrap_or(1);
        Self {
            patterns,
            pending: Zeroizing::new(vec![]),
            keep,
        }
    }

    pub(crate) fn text(&self, value: &str) -> String {
        let mut redactor = Self {
            patterns: self.patterns.clone(),
            pending: Zeroizing::new(vec![]),
            keep: self.keep,
        };
        String::from_utf8(redactor.push(value.as_bytes(), true)).expect("UTF-8 redaction")
    }

    pub(crate) fn push(&mut self, bytes: &[u8], final_chunk: bool) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        let mut output = vec![];
        let mut position = 0;
        while position < self.pending.len()
            && (final_chunk || self.pending.len() - position >= self.keep)
        {
            if let Some(pattern) = self
                .patterns
                .iter()
                .find(|pattern| self.pending[position..].starts_with(pattern))
            {
                output.extend_from_slice(b"[REDACTED]");
                position += pattern.len();
            } else {
                output.push(self.pending[position]);
                position += 1;
            }
        }
        self.pending.drain(..position);
        output
    }
}
