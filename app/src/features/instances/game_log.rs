//! Output of the game, kept per build for the log tab. Only the latest
//! session is kept, and only its last [`MAX_LINES`] lines.

use std::collections::VecDeque;

/// Lines kept per build; older ones drop off the top.
pub const MAX_LINES: usize = 5000;

#[derive(Debug, Default)]
pub struct GameLog {
    lines: VecDeque<String>,
    /// Grows with every change, so views can tell when to redraw.
    version: u64,
}

impl GameLog {
    pub fn push(&mut self, line: String) {
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line.trim_end().to_owned());
        self.version += 1;
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.version += 1;
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// The whole log, one line per line.
    pub fn text(&self) -> String {
        self.tail(self.lines.len())
    }

    /// The last `count` lines.
    pub fn tail(&self, count: usize) -> String {
        let skip = self.lines.len().saturating_sub(count);
        let mut text = String::new();
        for line in self.lines.iter().skip(skip) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(line);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_lines_and_counts_changes() {
        let mut log = GameLog::default();
        for index in 0..MAX_LINES + 2 {
            log.push(format!("line {index}\r\n"));
        }
        assert_eq!(
            log.tail(2),
            format!("line {}\nline {}", MAX_LINES, MAX_LINES + 1)
        );
        assert!(log.text().starts_with("line 2\n"));
        let version = log.version();
        log.clear();
        assert!(log.text().is_empty());
        assert!(log.version() > version);
    }
}
