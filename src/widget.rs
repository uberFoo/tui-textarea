use crate::textarea::TextArea;
use crate::tui::buffer::Buffer;
use crate::tui::layout::Rect;
use crate::tui::text::Text;
use crate::tui::widgets::{Paragraph, Widget};
use crate::util::{num_digits};

use ratatui::text::{Line, Span};
use std::cmp;
use std::sync::atomic::{AtomicU64, Ordering};

use syntect::{
    highlighting::{ThemeSet},
    parsing::SyntaxSet,
};

// &mut 'a (u16, u16, u16, u16) is not available since Renderer instance totally takes over the ownership of TextArea
// instance. In the case, the TextArea instance cannot be accessed from any other objects since it is mutablly
// borrowed.
//
// `tui::terminal::Frame::render_stateful_widget` would be an assumed way to render a stateful widget. But at this
// point we stick with using `tui::terminal::Frame::render_widget` because it is simpler API. Users don't need to
// manage states of textarea instances separately.
// https://docs.rs/tui/latest/tui/terminal/struct.Frame.html#method.render_stateful_widget
#[derive(Default)]
pub struct Viewport(AtomicU64);

impl Clone for Viewport {
    fn clone(&self) -> Self {
        let u = self.0.load(Ordering::Relaxed);
        Viewport(AtomicU64::new(u))
    }
}

impl Viewport {
    pub fn scroll_top(&self) -> (u16, u16) {
        let u = self.0.load(Ordering::Relaxed);
        ((u >> 16) as u16, u as u16)
    }

    pub fn rect(&self) -> (u16, u16, u16, u16) {
        let u = self.0.load(Ordering::Relaxed);
        let width = (u >> 48) as u16;
        let height = (u >> 32) as u16;
        let row = (u >> 16) as u16;
        let col = u as u16;
        (row, col, width, height)
    }

    pub fn position(&self) -> (u16, u16, u16, u16) {
        let (row_top, col_top, width, height) = self.rect();
        let row_bottom = row_top.saturating_add(height).saturating_sub(1);
        let col_bottom = col_top.saturating_add(width).saturating_sub(1);

        (
            row_top,
            col_top,
            cmp::max(row_top, row_bottom),
            cmp::max(col_top, col_bottom),
        )
    }

    fn store(&self, row: u16, col: u16, width: u16, height: u16) {
        // Pack four u16 values into one u64 value
        let u =
            ((width as u64) << 48) | ((height as u64) << 32) | ((row as u64) << 16) | col as u64;
        self.0.store(u, Ordering::Relaxed);
    }

    pub fn scroll(&mut self, rows: i16, cols: i16) {
        fn apply_scroll(pos: u16, delta: i16) -> u16 {
            if delta >= 0 {
                pos.saturating_add(delta as u16)
            } else {
                pos.saturating_sub(-delta as u16)
            }
        }

        let u = self.0.get_mut();
        let row = apply_scroll((*u >> 16) as u16, rows);
        let col = apply_scroll(*u as u16, cols);
        *u = (*u & 0xffff_ffff_0000_0000) | ((row as u64) << 16) | (col as u64);
    }
}

pub struct SyntaxRenderer<'a> {
    textarea: &'a TextArea<'a>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme: &'a str,
    // syntax: &'a SyntaxReference,
}

impl<'a> SyntaxRenderer<'a> {
    pub fn new(textarea: &'a TextArea<'a>, theme: &'a str) -> Self {
        let ps = SyntaxSet::load_defaults_nonewlines();
        let ts = ThemeSet::load_defaults();

        Self {
            textarea,
            syntax_set: ps,
            theme_set: ts,
            theme,
        }
    }

    #[inline]
    fn text(&self, _top_row: usize, _height: usize) -> Text<'a> {
        Text::default()
        // let syntax = self.syntax_set.find_syntax_by_extension("rs").unwrap();
        // let mut h = HighlightLines::new(syntax, &self.theme_set.themes[self.theme]);

        // let lines_len = self.textarea.lines().len();
        // let lnum_len = num_digits(lines_len);
        // let bottom_row = cmp::min(top_row + height, lines_len);
        // let mut lines = Vec::with_capacity(bottom_row - top_row);
        // for (i, line) in self.textarea.lines()[top_row..bottom_row]
        //     .iter()
        //     .enumerate()
        // {
        //     let ranges: Vec<(SyntectStyle, &str)> =
        //         h.highlight_line(line, &self.syntax_set).unwrap();
        //     let escaped = as_24_bit_terminal_escaped(&ranges[..], true);
        //     lines.push(escaped.into_spans().unwrap());
        //     // lines.push(self.textarea.syntax_line_spans(
        //     //     &mut h,
        //     //     &self.syntax_set,
        //     //     line.as_str(),
        //     //     top_row + i,
        //     //     lnum_len,
        //     // ));
        // }
        // Text::from(lines)
    }
}

impl<'a> Widget for SyntaxRenderer<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Rect { width, height, .. } = if let Some(b) = self.textarea.block() {
            b.inner(area)
        } else {
            area
        };

        fn next_scroll_top(prev_top: u16, cursor: u16, length: u16) -> u16 {
            if cursor < prev_top {
                cursor
            } else if prev_top + length <= cursor {
                cursor + 1 - length
            } else {
                prev_top
            }
        }

        let cursor = self.textarea.cursor();
        let (top_row, top_col) = self.textarea.viewport.scroll_top();
        let top_row = next_scroll_top(top_row, cursor.0 as u16, height);
        let top_col = next_scroll_top(top_col, cursor.1 as u16, width);

        let text = self.text(top_row as usize, height as usize);
        let mut inner = Paragraph::new(text)
            .style(self.textarea.style())
            .alignment(self.textarea.alignment());
        if let Some(b) = self.textarea.block() {
            inner = inner.block(b.clone());
        }
        if top_col != 0 {
            inner = inner.scroll((0, top_col));
        }

        // Store scroll top position for rendering on the next tick
        self.textarea
            .viewport
            .store(top_row, top_col, width, height);

        inner.render(area, buf);
    }
}

pub struct Renderer<'a>(&'a TextArea<'a>);

impl<'a> Renderer<'a> {
    pub fn new(textarea: &'a TextArea<'a>) -> Self {
        Self(textarea)
    }

    #[inline]
    fn text(&self, top_row: usize, height: usize) -> Text<'a> {
        let lines = &self.0.text().lines;
        let mut cursor = self.0.cursor();
        let cursor_style = self.0.cursor_style();
        // let style = self.0.cursor_line_style();
        let num_style = self.0.line_number_style();

        // let style = Style::default()
        //     .fg(Color::Yellow)
        //     .add_modifier(Modifier::ITALIC);
        // let mut raw_text = Text::raw("The first line\nThe second line");
        // let styled_text = Text::styled(String::from("The first line\nThe second line"), style);

        // raw_text.patch_style(style);
        let lines_len = self.0.lines().len();
        let lnum_len = num_digits(lines_len) as usize;
        cursor.1 += lnum_len + 1;
        let bottom_row = cmp::min(top_row + height, lines_len);

        // let row = cursor.0.clamp(top_row as usize, bottom_row as usize);
        // let row = cursor.0.clamp(top_row as usize, bottom_row as usize);
        // let row = cmp::min(row, lines.len() - 1);
        // log::debug!("br {bottom_row}");
        // let bottom_row = bottom_row - 3;

        // log::debug!(
        //     "cursor: ({},{}) height: {height} top_row: {top_row}, bottom_row: {bottom_row} min ({}), len {lines_len}",
        //     cursor.0,
        //     cursor.1,
        //     bottom_row.min(lines_len - 1)
        // );

        let mut text = Text::from(
            lines[top_row..bottom_row.min(lines.len())]
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    if let Some(style) = num_style {
                        let mut new_line = Line::from(Span::styled(
                            format!("{:lnum_len$} ", top_row + i + 1),
                            style,
                        ));
                        new_line.extend(line.clone().into_iter());
                        new_line
                    } else {
                        line.clone()
                    }
                })
                .collect::<Vec<_>>(),
        );
        // let foo = top_row -

        // txt.lines[cursor.0.min(height - 1)].patch_style(style);
        let roi = (cursor.0 - top_row).clamp(0, lines_len - 1);
        let mut i = 0;
        let mut j = 0;
        // let mut len = 0;
        let mut target_span = None;
        // log::debug!("roi: {}", roi);
        if text.lines.is_empty() {
            return text;
        }
        if roi == text.lines.len() {
            return text;
        }
        for span in &mut text.lines[roi].spans {
            i += span.content.len();
            if i >= cursor.1 {
                // len = span.content.len();
                target_span = Some(span);
                // log::debug!("i: {}, j: {}, {}", i, j, span.content);
                break;
            }
            j += 1
        }
        // break span j at column i into three. The one before the one, and the
        // one after. Unless it's just a single character. Then we are luckyy
        // we'll do that one first
        if let Some(span) = target_span.as_mut() {
            // log::warn!("span: {:?}", span);
            // if len == 1 {
            span.patch_style(cursor_style);
            // }
            // text.lines[row - top_row].patch_style(style);
        }
        // text.lines[row - top_row].patch_style(style);
        // text.patch_style(style);u
        text
        // let lines_len = self.0.lines().len();
        // let lnum_len = num_digits(lines_len);
        // let bottom_row = cmp::min(top_row + height, lines_len);
        // let mut lines = Vec::with_capacity(bottom_row - top_row);
        // for (i, line) in self.0.lines()[top_row..bottom_row].iter().enumerate() {
        //     lines.push(self.0.line_spans(line.as_str(), top_row + i, lnum_len));
        // }
        // Text::from(lines)
    }
}

impl<'a> Widget for Renderer<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Rect { width, height, .. } = if let Some(b) = self.0.block() {
            b.inner(area)
        } else {
            area
        };

        fn next_scroll_top(prev_top: u16, cursor: u16, length: u16) -> u16 {
            if cursor < prev_top {
                cursor
            } else if prev_top + length <= cursor {
                cursor + 1 - length
            } else {
                prev_top
            }
        }

        let cursor = self.0.cursor();
        let (top_row, top_col) = self.0.viewport.scroll_top();
        let top_row = next_scroll_top(top_row, cursor.0 as u16, height);
        let top_col = next_scroll_top(top_col, cursor.1 as u16, width);

        let text = self.text(top_row as usize, height as usize);
        let mut inner = Paragraph::new(text)
            .style(self.0.style())
            .alignment(self.0.alignment());
        if let Some(b) = self.0.block() {
            inner = inner.block(b.clone());
        }
        if top_col != 0 {
            inner = inner.scroll((0, top_col));
        }

        // Store scroll top position for rendering on the next tick
        self.0.viewport.store(top_row, top_col, width, height);

        inner.render(area, buf);
    }
}
