use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::renderer::cursor::CursorRenderer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PgField {
    Connection,
    Query,
    Results,
}

impl PgField {
    pub fn next(&self) -> Self {
        match self {
            PgField::Connection => PgField::Query,
            PgField::Query => PgField::Results,
            PgField::Results => PgField::Connection,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PgStatus {
    Disconnected,
    Connecting,
    Connected,
    Executing,
    Error(String),
}

impl PgStatus {
    pub fn color(&self) -> [f32; 4] {
        match self {
            PgStatus::Disconnected => [0.5, 0.5, 0.5, 1.0],
            PgStatus::Connecting => [0.95, 0.75, 0.25, 1.0],
            PgStatus::Connected => [0.4, 0.85, 0.5, 1.0],
            PgStatus::Executing => [0.95, 0.75, 0.25, 1.0],
            PgStatus::Error(_) => [0.95, 0.4, 0.4, 1.0],
        }
    }

    pub fn label(&self) -> &str {
        match self {
            PgStatus::Disconnected => "disconnected",
            PgStatus::Connecting => "connecting...",
            PgStatus::Connected => "connected",
            PgStatus::Executing => "executing...",
            PgStatus::Error(_) => "error",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ScrollbarDrag {
    Vertical { start_mouse_y: f32, start_scroll: usize },
    Horizontal { start_mouse_x: f32, start_scroll: usize },
}

pub struct PgQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub exec_time_ms: u64,
}

pub struct PostgresPane {
    // Connection
    pub connection_string: String,
    pub conn_cursor: usize,
    pub status: PgStatus,

    // Query editor
    pub query: String,
    pub query_cursor: usize,

    // Results
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub exec_time_ms: Option<u64>,
    pub error: Option<String>,

    // UI state
    pub focus_field: PgField,
    pub results_scroll: usize,
    pub results_scroll_x: usize, // horizontal scroll in chars
    pub col_widths: Vec<usize>, // char widths for each column
    pub dragging_scrollbar: Option<ScrollbarDrag>,

    pub toast_message: Option<String>,
    pub toast_time: Option<Instant>,
    pub cursor_renderer: CursorRenderer,

    pending_connect: Arc<Mutex<Option<Result<(), String>>>>,
    pending_query: Arc<Mutex<Option<Result<PgQueryResult, String>>>>,
    // Keep the connection alive across queries
    connection: Arc<Mutex<Option<postgres::Client>>>,
}

impl PostgresPane {
    pub fn new(cursor_blink: bool) -> Self {
        Self {
            connection_string: String::new(),
            conn_cursor: 0,
            status: PgStatus::Disconnected,

            query: String::new(),
            query_cursor: 0,

            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            exec_time_ms: None,
            error: None,

            focus_field: PgField::Connection,
            results_scroll: 0,
            results_scroll_x: 0,
            col_widths: Vec::new(),
            dragging_scrollbar: None,

            toast_message: None,
            toast_time: None,
            cursor_renderer: CursorRenderer::new(cursor_blink),

            pending_connect: Arc::new(Mutex::new(None)),
            pending_query: Arc::new(Mutex::new(None)),
            connection: Arc::new(Mutex::new(None)),
        }
    }

    /// Try to parse a postgres connection string from pasted text.
    pub fn try_parse_connection_string(&mut self, text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.starts_with("postgres://") || trimmed.starts_with("postgresql://") {
            self.connection_string = trimmed.to_string();
            self.conn_cursor = self.connection_string.len();
            self.focus_field = PgField::Connection;
            true
        } else {
            false
        }
    }

    pub fn connect(&mut self) {
        if self.connection_string.is_empty() {
            return;
        }
        if matches!(self.status, PgStatus::Connecting) {
            return;
        }
        self.status = PgStatus::Connecting;
        self.error = None;

        let conn_str = self.connection_string.clone();
        let result = Arc::clone(&self.pending_connect);
        let connection = Arc::clone(&self.connection);

        std::thread::spawn(move || {
            // Try with native TLS first (accepts self-signed certs), fall back to NoTls
            let connect_result = {
                let tls_builder = native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build();
                match tls_builder {
                    Ok(connector) => {
                        let tls = postgres_native_tls::MakeTlsConnector::new(connector);
                        postgres::Client::connect(&conn_str, tls)
                            .or_else(|_| postgres::Client::connect(&conn_str, postgres::NoTls))
                    }
                    Err(_) => postgres::Client::connect(&conn_str, postgres::NoTls),
                }
            };

            match connect_result {
                Ok(client) => {
                    if let Ok(mut lock) = connection.lock() {
                        *lock = Some(client);
                    }
                    if let Ok(mut lock) = result.lock() {
                        *lock = Some(Ok(()));
                    }
                }
                Err(e) => {
                    // Build a detailed error message including the cause chain
                    let mut msg = format!("{}", e);
                    let mut source: Option<&dyn std::error::Error> = std::error::Error::source(&e);
                    while let Some(cause) = source {
                        msg.push_str(&format!(" — {}", cause));
                        source = std::error::Error::source(cause);
                    }
                    if let Ok(mut lock) = result.lock() {
                        *lock = Some(Err(msg));
                    }
                }
            }
        });
    }

    pub fn execute_query(&mut self) {
        if self.query.trim().is_empty() {
            return;
        }
        if !matches!(self.status, PgStatus::Connected) {
            return;
        }

        self.status = PgStatus::Executing;
        self.error = None;
        self.columns.clear();
        self.rows.clear();
        self.col_widths.clear();
        self.exec_time_ms = None;
        self.results_scroll = 0;
        self.results_scroll_x = 0;

        let query = self.query.clone();
        let result = Arc::clone(&self.pending_query);
        let connection = Arc::clone(&self.connection);

        std::thread::spawn(move || {
            let start = Instant::now();
            let res = {
                let mut lock = match connection.lock() {
                    Ok(l) => l,
                    Err(_) => {
                        if let Ok(mut r) = result.lock() {
                            *r = Some(Err("Failed to acquire connection lock".to_string()));
                        }
                        return;
                    }
                };
                let client = match lock.as_mut() {
                    Some(c) => c,
                    None => {
                        if let Ok(mut r) = result.lock() {
                            *r = Some(Err("Not connected".to_string()));
                        }
                        return;
                    }
                };
                // Use simple_query to get all values as text (handles JSONB, timestamps, etc.)
                client.simple_query(&query)
            };
            let elapsed = start.elapsed().as_millis() as u64;

            match res {
                Ok(messages) => {
                    let mut columns: Vec<String> = Vec::new();
                    let mut row_data: Vec<Vec<String>> = Vec::new();

                    for msg in &messages {
                        match msg {
                            postgres::SimpleQueryMessage::Row(row) => {
                                // Extract column names from first row
                                if columns.is_empty() {
                                    columns = row.columns().iter()
                                        .map(|c| c.name().to_string())
                                        .collect();
                                }
                                let vals: Vec<String> = (0..row.columns().len())
                                    .map(|i| row.get(i).unwrap_or("NULL").to_string())
                                    .collect();
                                row_data.push(vals);
                            }
                            postgres::SimpleQueryMessage::CommandComplete(count) => {
                                // For INSERT/UPDATE/DELETE, show affected rows
                                if columns.is_empty() && row_data.is_empty() {
                                    columns = vec!["result".to_string()];
                                    row_data.push(vec![format!("{} rows affected", count)]);
                                }
                            }
                            _ => {}
                        }
                    }

                    let row_count = row_data.len();
                    if let Ok(mut lock) = result.lock() {
                        *lock = Some(Ok(PgQueryResult {
                            columns,
                            rows: row_data,
                            row_count,
                            exec_time_ms: elapsed,
                        }));
                    }
                }
                Err(e) => {
                    if let Ok(mut lock) = result.lock() {
                        *lock = Some(Err(e.to_string()));
                    }
                }
            }
        });
    }

    /// Poll for completed connect/query results. Call each frame.
    pub fn poll(&mut self) {
        // Poll connect
        if matches!(self.status, PgStatus::Connecting) {
            if let Ok(mut lock) = self.pending_connect.lock() {
                if let Some(result) = lock.take() {
                    match result {
                        Ok(()) => {
                            self.status = PgStatus::Connected;
                            self.focus_field = PgField::Query;
                        }
                        Err(e) => {
                            self.status = PgStatus::Error(e.clone());
                            self.error = Some(e);
                        }
                    }
                }
            }
        }

        // Poll query
        if matches!(self.status, PgStatus::Executing) {
            let result = if let Ok(mut lock) = self.pending_query.lock() {
                lock.take()
            } else {
                None
            };
            if let Some(result) = result {
                match result {
                    Ok(qr) => {
                        self.status = PgStatus::Connected;
                        self.columns = qr.columns;
                        self.rows = qr.rows;
                        self.row_count = qr.row_count;
                        self.exec_time_ms = Some(qr.exec_time_ms);
                        self.calculate_col_widths();
                        self.focus_field = PgField::Results;
                    }
                    Err(e) => {
                        self.status = PgStatus::Connected;
                        self.error = Some(e);
                    }
                }
            }
        }
    }

    fn calculate_col_widths(&mut self) {
        self.col_widths = self.columns.iter().enumerate().map(|(i, col)| {
            let header_w = col.chars().count();
            let max_data_w = self.rows.iter()
                .map(|row| row.get(i).map(|v| v.chars().count()).unwrap_or(0))
                .max()
                .unwrap_or(0);
            header_w.max(max_data_w).min(40) // cap at 40 chars
        }).collect();
    }

    pub fn result_line_count(&self) -> usize {
        self.rows.len() + if self.columns.is_empty() { 0 } else { 2 } // header + separator + rows
    }

    /// Total width of all columns in chars (including gaps).
    pub fn total_table_width(&self) -> usize {
        let col_gap = 2;
        if self.col_widths.is_empty() {
            0
        } else {
            self.col_widths.iter().sum::<usize>() + (self.col_widths.len() - 1) * col_gap
        }
    }

    // ── Connection field input ──

    pub fn insert_at_conn(&mut self, text: &str) {
        self.connection_string.insert_str(self.conn_cursor, text);
        self.conn_cursor += text.len();
    }

    pub fn backspace_conn(&mut self) {
        if self.conn_cursor > 0 {
            let prev = self.connection_string[..self.conn_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
            self.connection_string.drain(prev..self.conn_cursor);
            self.conn_cursor = prev;
        }
    }

    pub fn move_conn_left(&mut self) {
        if self.conn_cursor > 0 {
            self.conn_cursor = self.connection_string[..self.conn_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
        }
    }

    pub fn move_conn_right(&mut self) {
        if self.conn_cursor < self.connection_string.len() {
            self.conn_cursor = self.connection_string[self.conn_cursor..]
                .char_indices().nth(1)
                .map(|(i, _)| self.conn_cursor + i)
                .unwrap_or(self.connection_string.len());
        }
    }

    pub fn move_conn_word_left(&mut self) {
        if self.conn_cursor == 0 { return; }
        let before = &self.connection_string[..self.conn_cursor];
        let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
        if trimmed.is_empty() { self.conn_cursor = 0; return; }
        self.conn_cursor = trimmed.rfind(|c: char| !c.is_alphanumeric())
            .map(|i| i + 1).unwrap_or(0);
    }

    pub fn move_conn_word_right(&mut self) {
        if self.conn_cursor >= self.connection_string.len() { return; }
        let after = &self.connection_string[self.conn_cursor..];
        let skip = after.find(|c: char| !c.is_alphanumeric()).unwrap_or(after.len());
        let rest = &after[skip..];
        let skip2 = rest.find(|c: char| c.is_alphanumeric()).unwrap_or(rest.len());
        self.conn_cursor += skip + skip2;
    }

    // ── Query editor input ──

    pub fn insert_at_query(&mut self, text: &str) {
        self.query.insert_str(self.query_cursor, text);
        self.query_cursor += text.len();
    }

    pub fn backspace_query(&mut self) {
        if self.query_cursor > 0 {
            let prev = self.query[..self.query_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
            self.query.drain(prev..self.query_cursor);
            self.query_cursor = prev;
        }
    }

    pub fn move_query_left(&mut self) {
        if self.query_cursor > 0 {
            self.query_cursor = self.query[..self.query_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
        }
    }

    pub fn move_query_right(&mut self) {
        if self.query_cursor < self.query.len() {
            self.query_cursor = self.query[self.query_cursor..]
                .char_indices().nth(1)
                .map(|(i, _)| self.query_cursor + i)
                .unwrap_or(self.query.len());
        }
    }

    pub fn move_query_word_left(&mut self) {
        if self.query_cursor == 0 { return; }
        let before = &self.query[..self.query_cursor];
        let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
        if trimmed.is_empty() { self.query_cursor = 0; return; }
        self.query_cursor = trimmed.rfind(|c: char| !c.is_alphanumeric())
            .map(|i| i + 1).unwrap_or(0);
    }

    pub fn move_query_word_right(&mut self) {
        if self.query_cursor >= self.query.len() { return; }
        let after = &self.query[self.query_cursor..];
        let skip = after.find(|c: char| !c.is_alphanumeric()).unwrap_or(after.len());
        let rest = &after[skip..];
        let skip2 = rest.find(|c: char| c.is_alphanumeric()).unwrap_or(rest.len());
        self.query_cursor += skip + skip2;
    }

    pub fn move_query_line_start(&mut self) {
        let before = &self.query[..self.query_cursor];
        self.query_cursor = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    }

    pub fn move_query_line_end(&mut self) {
        let after = &self.query[self.query_cursor..];
        self.query_cursor += after.find('\n').unwrap_or(after.len());
    }

    pub fn move_query_up(&mut self) {
        let before = &self.query[..self.query_cursor];
        let last_nl = before.rfind('\n');
        let Some(nl_pos) = last_nl else {
            self.query_cursor = 0;
            return;
        };
        let col = before[nl_pos + 1..].chars().count();
        let prev_line_start = self.query[..nl_pos].rfind('\n')
            .map(|p| p + 1).unwrap_or(0);
        let prev_line = &self.query[prev_line_start..nl_pos];
        let prev_line_chars = prev_line.chars().count();
        let target_col = col.min(prev_line_chars);
        self.query_cursor = prev_line.char_indices()
            .nth(target_col)
            .map(|(i, _)| prev_line_start + i)
            .unwrap_or(nl_pos);
    }

    pub fn move_query_down(&mut self) {
        let after_cursor = &self.query[self.query_cursor..];
        let next_nl = after_cursor.find('\n');
        let Some(nl_offset) = next_nl else {
            self.query_cursor = self.query.len();
            return;
        };
        let before = &self.query[..self.query_cursor];
        let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let col = before[line_start..].chars().count();
        let next_line_start = self.query_cursor + nl_offset + 1;
        if next_line_start > self.query.len() {
            self.query_cursor = self.query.len();
            return;
        }
        let next_line_end = self.query[next_line_start..].find('\n')
            .map(|p| next_line_start + p)
            .unwrap_or(self.query.len());
        let next_line = &self.query[next_line_start..next_line_end];
        let next_line_chars = next_line.chars().count();
        let target_col = col.min(next_line_chars);
        self.query_cursor = next_line.char_indices()
            .nth(target_col)
            .map(|(i, _)| next_line_start + i)
            .unwrap_or(next_line_end);
    }

    /// Mask password in connection string for display.
    pub fn display_connection_string(&self) -> String {
        // Mask the password portion: postgres://user:PASSWORD@host
        if let Some(at_pos) = self.connection_string.find('@') {
            if let Some(colon_pos) = self.connection_string[..at_pos].rfind(':') {
                // Check there's a // before, meaning this is user:pass
                if let Some(slash_pos) = self.connection_string.find("//") {
                    if colon_pos > slash_pos + 2 {
                        let pass_len = at_pos - colon_pos - 1;
                        let masked = "*".repeat(pass_len);
                        return format!("{}:{}{}",
                            &self.connection_string[..colon_pos],
                            masked,
                            &self.connection_string[at_pos..]);
                    }
                }
            }
        }
        self.connection_string.clone()
    }

    pub fn show_toast(&mut self, msg: &str) {
        self.toast_message = Some(msg.to_string());
        self.toast_time = Some(Instant::now());
    }

    pub fn toast_visible(&self) -> bool {
        self.toast_time.map(|t| t.elapsed().as_millis() < 1500).unwrap_or(false)
    }
}

