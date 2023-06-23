use crate::tui::style::Style;
use crate::tui::text::{Line, Span};
use crate::util::{num_digits, spaces};
use std::borrow::Cow;
use std::cmp::Ordering;

enum Boundary {
    Cursor(Style),
    #[cfg(feature = "search")]
    Search(Style),
    End,
}

impl Boundary {
    fn cmp(&self, other: &Boundary) -> Ordering {
        fn rank(b: &Boundary) -> u8 {
            match b {
                Boundary::Cursor(_) => 2,
                #[cfg(feature = "search")]
                Boundary::Search(_) => 1,
                Boundary::End => 0,
            }
        }
        rank(self).cmp(&rank(other))
    }

    fn style(&self) -> Option<Style> {
        match self {
            Boundary::Cursor(s) => Some(*s),
            #[cfg(feature = "search")]
            Boundary::Search(s) => Some(*s),
            Boundary::End => None,
        }
    }
}

fn replace_tabs(s: &str, tab_len: u8) -> Cow<'_, str> {
    let tab = spaces(tab_len);
    let mut buf = String::new();
    for (i, c) in s.char_indices() {
        if buf.is_empty() {
            if c == '\t' {
                buf.reserve(s.len());
                buf.push_str(&s[..i]);
                buf.push_str(tab);
            }
        } else if c == '\t' {
            buf.push_str(tab);
        } else {
            buf.push(c);
        }
    }
    if buf.is_empty() {
        Cow::Borrowed(s)
    } else {
        Cow::Owned(buf)
    }
}

pub struct LineHighlighter<'a> {
    line: &'a str,
    spans: Vec<Span<'a>>,
    boundaries: Vec<(Boundary, usize)>, // TODO: Consider smallvec
    style_begin: Style,
    cursor_at_end: bool,
    cursor_style: Style,
    tab_len: u8,
}

impl<'a> LineHighlighter<'a> {
    pub fn new(line: &'a str, cursor_style: Style, tab_len: u8) -> Self {
        Self {
            line,
            spans: vec![],
            boundaries: vec![],
            style_begin: Style::default(),
            cursor_at_end: false,
            cursor_style,
            tab_len,
        }
    }

    pub fn line_number(&mut self, row: usize, lnum_len: u8, style: Style) {
        let pad = spaces(lnum_len - num_digits(row + 1) + 1);
        self.spans
            .push(Span::styled(format!("{}{} ", pad, row + 1), style));
    }

    pub fn push_spans(&mut self, spans: impl IntoIterator<Item = Span<'a>>) {
        self.spans.extend(spans);
    }

    pub fn cursor_line(&mut self, cursor_col: usize, style: Style) {
        if let Some((start, c)) = self.line.char_indices().nth(cursor_col) {
            self.boundaries
                .push((Boundary::Cursor(self.cursor_style), start));
            self.boundaries.push((Boundary::End, start + c.len_utf8()));
        } else {
            self.cursor_at_end = true;
        }
        self.style_begin = style;
    }

    #[cfg(feature = "search")]
    pub fn search(&mut self, matches: impl Iterator<Item = (usize, usize)>, style: Style) {
        for (start, end) in matches {
            if start != end {
                self.boundaries.push((Boundary::Search(style), start));
                self.boundaries.push((Boundary::End, end));
            }
        }
    }

    pub fn into_spans(self) -> Line<'a> {
        let Self {
            line,
            mut spans,
            mut boundaries,
            tab_len,
            style_begin,
            cursor_style,
            cursor_at_end,
        } = self;

        if boundaries.is_empty() {
            spans.push(Span::styled(replace_tabs(line, tab_len), style_begin));
            if cursor_at_end {
                spans.push(Span::styled(" ", cursor_style));
            }
            return Line::from(spans);
        }

        boundaries.sort_unstable_by(|(l, i), (r, j)| match i.cmp(j) {
            Ordering::Equal => l.cmp(r),
            o => o,
        });

        let mut boundaries = boundaries.into_iter();
        let mut style = style_begin;
        let mut start = 0;
        let mut stack = vec![];

        loop {
            if let Some((next_boundary, end)) = boundaries.next() {
                if start < end {
                    spans.push(Span::styled(
                        replace_tabs(&line[start..end], tab_len),
                        style,
                    ));
                }

                style = if let Some(s) = next_boundary.style() {
                    stack.push(style);
                    s
                } else {
                    stack.pop().unwrap_or(style_begin)
                };
                start = end;
            } else {
                if start != line.len() {
                    spans.push(Span::styled(replace_tabs(&line[start..], tab_len), style));
                }
                if cursor_at_end {
                    spans.push(Span::styled(" ", cursor_style));
                }
                return Line::from(spans);
            }
        }
    }
}
