use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::renderer::cursor::CursorRenderer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
            HttpMethod::PUT => "PUT",
            HttpMethod::DELETE => "DELETE",
            HttpMethod::PATCH => "PATCH",
            HttpMethod::HEAD => "HEAD",
            HttpMethod::OPTIONS => "OPTIONS",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "POST" => HttpMethod::POST,
            "PUT" => HttpMethod::PUT,
            "DELETE" => HttpMethod::DELETE,
            "PATCH" => HttpMethod::PATCH,
            "HEAD" => HttpMethod::HEAD,
            "OPTIONS" => HttpMethod::OPTIONS,
            _ => HttpMethod::GET,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            HttpMethod::GET => HttpMethod::POST,
            HttpMethod::POST => HttpMethod::PUT,
            HttpMethod::PUT => HttpMethod::DELETE,
            HttpMethod::DELETE => HttpMethod::PATCH,
            HttpMethod::PATCH => HttpMethod::HEAD,
            HttpMethod::HEAD => HttpMethod::OPTIONS,
            HttpMethod::OPTIONS => HttpMethod::GET,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            HttpMethod::GET => HttpMethod::OPTIONS,
            HttpMethod::POST => HttpMethod::GET,
            HttpMethod::PUT => HttpMethod::POST,
            HttpMethod::DELETE => HttpMethod::PUT,
            HttpMethod::PATCH => HttpMethod::DELETE,
            HttpMethod::HEAD => HttpMethod::PATCH,
            HttpMethod::OPTIONS => HttpMethod::HEAD,
        }
    }

    pub fn color(&self) -> [f32; 4] {
        match self {
            HttpMethod::GET => [0.4, 0.8, 0.5, 1.0],
            HttpMethod::POST => [0.95, 0.75, 0.25, 1.0],
            HttpMethod::PUT => [0.4, 0.6, 0.95, 1.0],
            HttpMethod::DELETE => [0.95, 0.4, 0.4, 1.0],
            HttpMethod::PATCH => [0.75, 0.5, 0.95, 1.0],
            HttpMethod::HEAD => [0.6, 0.6, 0.6, 1.0],
            HttpMethod::OPTIONS => [0.6, 0.6, 0.6, 1.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HttpField {
    Method,
    Url,
    Headers,
    Body,
    Response,
    Search,
}

impl HttpField {
    pub fn next(&self) -> Self {
        match self {
            HttpField::Method => HttpField::Url,
            HttpField::Url => HttpField::Headers,
            HttpField::Headers => HttpField::Body,
            HttpField::Body => HttpField::Method,
            HttpField::Response => HttpField::Method,
            HttpField::Search => HttpField::Search, // stay in search
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResponseView {
    Raw,
    Tree,
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub time_ms: u64,
}

/// A flattened line in the JSON tree view.
pub struct TreeLine {
    pub depth: usize,
    pub key: Option<String>,
    pub preview: String,       // value preview or "{...}" / "[...]"
    pub is_expandable: bool,
    pub expanded: bool,
    pub path: String,          // dot-separated path for tracking collapse state
    pub value_kind: TreeValueKind,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TreeValueKind {
    Object,
    Array,
    String,
    Number,
    Boolean,
    Null,
}

pub struct HttpPane {
    pub method: HttpMethod,
    pub url: String,
    pub url_cursor: usize,
    pub headers: Vec<(String, String)>,
    pub header_edit_index: usize,
    pub header_edit_field: usize, // 0=key, 1=value
    pub header_cursor: usize,    // byte offset within current key or value
    pub body: String,
    pub body_cursor: usize,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: String,
    pub response_time_ms: Option<u64>,
    pub focus_field: HttpField,
    pub loading: bool,
    pub error: Option<String>,
    pub scroll_offset: usize,
    pub response_view: ResponseView,
    pub tree_collapsed: std::collections::HashSet<String>,
    pub tree_scroll: usize,
    pub search_query: String,
    pub search_cursor: usize,
    pub search_active: bool,
    pub search_matches: Vec<(usize, usize)>, // (line_index, col_start)
    pub search_current: usize,               // current match index
    pub toast_message: Option<String>,
    pub toast_time: Option<Instant>,
    pub cursor_renderer: CursorRenderer,
    pending_response: Arc<Mutex<Option<Result<HttpResponse, String>>>>,
}

impl HttpPane {
    pub fn new(cursor_blink: bool) -> Self {
        Self {
            method: HttpMethod::GET,
            url: String::new(),
            url_cursor: 0,
            headers: Vec::new(),
            header_edit_index: 0,
            header_edit_field: 0,
            header_cursor: 0,
            body: String::new(),
            body_cursor: 0,
            response_status: None,
            response_headers: Vec::new(),
            response_body: String::new(),
            response_time_ms: None,
            focus_field: HttpField::Url,
            loading: false,
            error: None,
            scroll_offset: 0,
            response_view: ResponseView::Raw,
            tree_collapsed: std::collections::HashSet::new(),
            tree_scroll: 0,
            search_query: String::new(),
            search_cursor: 0,
            search_active: false,
            search_matches: Vec::new(),
            search_current: 0,
            toast_message: None,
            toast_time: None,
            cursor_renderer: CursorRenderer::new(cursor_blink),
            pending_response: Arc::new(Mutex::new(None)),
        }
    }

    /// Try to parse a curl command and populate fields. Returns true if parsed.
    pub fn try_parse_curl(&mut self, text: &str) -> bool {
        let trimmed = text.trim();
        // Detect curl command: starts with "curl" optionally followed by whitespace
        let starts_curl = trimmed.starts_with("curl ")
            || trimmed.starts_with("curl\t")
            || trimmed.starts_with("curl\n")
            || trimmed.starts_with("curl\r");
        if !starts_curl {
            return false;
        }

        // Join continuation lines (backslash + newline patterns)
        let joined = trimmed
            .replace("\\\r\n", " ")
            .replace("\\\n", " ")
            .replace("\r\n", " ")
            .replace('\n', " ");
        let tokens = shell_tokenize(&joined);

        let mut url = String::new();
        let mut method = None;
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut body = String::new();

        let mut i = 1; // skip "curl"
        while i < tokens.len() {
            let tok = &tokens[i];
            match tok.as_str() {
                "-X" | "--request" => {
                    if i + 1 < tokens.len() {
                        method = Some(HttpMethod::from_str(&tokens[i + 1]));
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "-H" | "--header" => {
                    if i + 1 < tokens.len() {
                        let h = &tokens[i + 1];
                        if let Some(colon) = h.find(':') {
                            let key = h[..colon].trim().to_string();
                            let val = h[colon + 1..].trim().to_string();
                            headers.push((key, val));
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "-d" | "--data" | "--data-raw" | "--data-binary" => {
                    if i + 1 < tokens.len() {
                        body = tokens[i + 1].clone();
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                s if s.starts_with('-') => {
                    // Skip unknown flags; consume next token if it looks like a value
                    if i + 1 < tokens.len() && !tokens[i + 1].starts_with('-') {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                _ => {
                    // Bare argument = URL
                    if url.is_empty() {
                        url = tok.clone();
                    }
                    i += 1;
                }
            }
        }

        if url.is_empty() {
            return false;
        }

        // Infer method from body if not explicit
        let final_method = method.unwrap_or(if body.is_empty() {
            HttpMethod::GET
        } else {
            HttpMethod::POST
        });

        self.method = final_method;
        self.url = url;
        self.url_cursor = self.url.len();
        self.headers = headers;
        self.body = body;
        self.body_cursor = self.body.len();
        // Pretty-print body if JSON
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&self.body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                self.body = pretty;
                self.body_cursor = self.body.len();
            }
        }
        self.focus_field = HttpField::Url;
        true
    }

    pub fn send_request(&mut self) {
        if self.url.is_empty() || self.loading {
            return;
        }
        self.loading = true;
        self.error = None;
        self.response_status = None;
        self.response_headers.clear();
        self.response_body.clear();
        self.response_time_ms = None;
        self.scroll_offset = 0;

        let method = self.method;
        let url = self.url.clone();
        let headers = self.headers.clone();
        let body = self.body.clone();
        let result = Arc::clone(&self.pending_response);

        std::thread::spawn(move || {
            let start = Instant::now();
            let res = Self::do_request(method, &url, &headers, &body);
            let elapsed = start.elapsed().as_millis() as u64;

            let response = match res {
                Ok((status, resp_headers, resp_body)) => Ok(HttpResponse {
                    status,
                    headers: resp_headers,
                    body: resp_body,
                    time_ms: elapsed,
                }),
                Err(e) => Err(e),
            };

            if let Ok(mut lock) = result.lock() {
                *lock = Some(response);
            }
        });
    }

    fn do_request(
        method: HttpMethod,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<(u16, Vec<(String, String)>, String), String> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .new_agent();

        fn add_headers_nb(mut req: ureq::RequestBuilder<ureq::typestate::WithoutBody>, headers: &[(String, String)]) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
            for (k, v) in headers {
                if !k.is_empty() && !v.is_empty() {
                    req = req.header(k.as_str(), v.as_str());
                }
            }
            req
        }

        fn add_headers_wb(mut req: ureq::RequestBuilder<ureq::typestate::WithBody>, headers: &[(String, String)]) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
            for (k, v) in headers {
                if !k.is_empty() && !v.is_empty() {
                    req = req.header(k.as_str(), v.as_str());
                }
            }
            req
        }

        let has_content_type = headers.iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

        let response: Result<ureq::http::Response<ureq::Body>, ureq::Error> = match method {
            HttpMethod::GET => add_headers_nb(agent.get(url), headers).call(),
            HttpMethod::DELETE => add_headers_nb(agent.delete(url), headers).call(),
            HttpMethod::HEAD => add_headers_nb(agent.head(url), headers).call(),
            HttpMethod::OPTIONS => add_headers_nb(agent.options(url), headers).call(),
            HttpMethod::POST => {
                let req = add_headers_wb(agent.post(url), headers);
                if body.is_empty() {
                    req.send_empty()
                } else if has_content_type {
                    req.send(body.as_bytes())
                } else {
                    req.content_type("application/json").send(body.as_bytes())
                }
            }
            HttpMethod::PUT => {
                let req = add_headers_wb(agent.put(url), headers);
                if body.is_empty() {
                    req.send_empty()
                } else if has_content_type {
                    req.send(body.as_bytes())
                } else {
                    req.content_type("application/json").send(body.as_bytes())
                }
            }
            HttpMethod::PATCH => {
                let req = add_headers_wb(agent.patch(url), headers);
                if body.is_empty() {
                    req.send_empty()
                } else if has_content_type {
                    req.send(body.as_bytes())
                } else {
                    req.content_type("application/json").send(body.as_bytes())
                }
            }
        };

        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let resp_headers: Vec<(String, String)> = resp.headers().iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let mut body_reader = resp.into_body();
                let resp_body = body_reader.read_to_string().unwrap_or_default();
                let pretty_body = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp_body) {
                    serde_json::to_string_pretty(&json).unwrap_or(resp_body)
                } else {
                    resp_body
                };
                Ok((status, resp_headers, pretty_body))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Poll for completed HTTP response. Call this each frame.
    pub fn poll_response(&mut self) {
        if !self.loading {
            return;
        }
        let mut lock = match self.pending_response.lock() {
            Ok(l) => l,
            Err(_) => return,
        };
        if let Some(result) = lock.take() {
            self.loading = false;
            match result {
                Ok(resp) => {
                    self.response_status = Some(resp.status);
                    self.response_headers = resp.headers;
                    self.response_body = resp.body;
                    self.response_time_ms = Some(resp.time_ms);
                    self.focus_field = HttpField::Response;
                }
                Err(e) => {
                    self.error = Some(e);
                }
            }
        }
    }

    // Input handling helpers

    pub fn insert_at_url(&mut self, text: &str) {
        self.url.insert_str(self.url_cursor, text);
        self.url_cursor += text.len();
    }

    pub fn backspace_url(&mut self) {
        if self.url_cursor > 0 {
            let prev = self.url[..self.url_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
            self.url.drain(prev..self.url_cursor);
            self.url_cursor = prev;
        }
    }

    pub fn move_url_left(&mut self) {
        if self.url_cursor > 0 {
            self.url_cursor = self.url[..self.url_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
        }
    }

    pub fn move_url_right(&mut self) {
        if self.url_cursor < self.url.len() {
            self.url_cursor = self.url[self.url_cursor..]
                .char_indices().nth(1)
                .map(|(i, _)| self.url_cursor + i)
                .unwrap_or(self.url.len());
        }
    }

    pub fn move_url_word_left(&mut self) {
        if self.url_cursor == 0 { return; }
        let before = &self.url[..self.url_cursor];
        let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
        if trimmed.is_empty() { self.url_cursor = 0; return; }
        self.url_cursor = trimmed.rfind(|c: char| !c.is_alphanumeric())
            .map(|i| i + 1).unwrap_or(0);
    }

    pub fn move_url_word_right(&mut self) {
        if self.url_cursor >= self.url.len() { return; }
        let after = &self.url[self.url_cursor..];
        // Skip current word chars
        let skip = after.find(|c: char| !c.is_alphanumeric()).unwrap_or(after.len());
        let rest = &after[skip..];
        // Skip separators
        let skip2 = rest.find(|c: char| c.is_alphanumeric()).unwrap_or(rest.len());
        self.url_cursor += skip + skip2;
    }

    pub fn insert_at_body(&mut self, text: &str) {
        self.body.insert_str(self.body_cursor, text);
        self.body_cursor += text.len();
    }

    pub fn backspace_body(&mut self) {
        if self.body_cursor > 0 {
            let prev = self.body[..self.body_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
            self.body.drain(prev..self.body_cursor);
            self.body_cursor = prev;
        }
    }

    pub fn move_body_left(&mut self) {
        if self.body_cursor > 0 {
            self.body_cursor = self.body[..self.body_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
        }
    }

    pub fn move_body_right(&mut self) {
        if self.body_cursor < self.body.len() {
            self.body_cursor = self.body[self.body_cursor..]
                .char_indices().nth(1)
                .map(|(i, _)| self.body_cursor + i)
                .unwrap_or(self.body.len());
        }
    }

    pub fn move_body_word_left(&mut self) {
        if self.body_cursor == 0 { return; }
        let before = &self.body[..self.body_cursor];
        let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
        if trimmed.is_empty() { self.body_cursor = 0; return; }
        self.body_cursor = trimmed.rfind(|c: char| !c.is_alphanumeric())
            .map(|i| i + 1).unwrap_or(0);
    }

    pub fn move_body_word_right(&mut self) {
        if self.body_cursor >= self.body.len() { return; }
        let after = &self.body[self.body_cursor..];
        let skip = after.find(|c: char| !c.is_alphanumeric()).unwrap_or(after.len());
        let rest = &after[skip..];
        let skip2 = rest.find(|c: char| c.is_alphanumeric()).unwrap_or(rest.len());
        self.body_cursor += skip + skip2;
    }

    pub fn move_body_line_start(&mut self) {
        let before = &self.body[..self.body_cursor];
        self.body_cursor = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    }

    pub fn move_body_line_end(&mut self) {
        let after = &self.body[self.body_cursor..];
        self.body_cursor += after.find('\n').unwrap_or(after.len());
    }

    pub fn move_body_up(&mut self) {
        let before = &self.body[..self.body_cursor];
        let last_nl = before.rfind('\n');
        let Some(nl_pos) = last_nl else {
            // Already on first line, move to start
            self.body_cursor = 0;
            return;
        };
        // Current column (chars from last newline to cursor)
        let col = before[nl_pos + 1..].chars().count();
        // Find the start of the previous line
        let prev_line_start = self.body[..nl_pos].rfind('\n')
            .map(|p| p + 1).unwrap_or(0);
        let prev_line = &self.body[prev_line_start..nl_pos];
        let prev_line_chars = prev_line.chars().count();
        let target_col = col.min(prev_line_chars);
        // Convert char offset to byte offset
        self.body_cursor = prev_line.char_indices()
            .nth(target_col)
            .map(|(i, _)| prev_line_start + i)
            .unwrap_or(nl_pos); // end of prev line
    }

    pub fn move_body_down(&mut self) {
        let after_cursor = &self.body[self.body_cursor..];
        let next_nl = after_cursor.find('\n');
        let Some(nl_offset) = next_nl else {
            // Already on last line, move to end
            self.body_cursor = self.body.len();
            return;
        };
        // Current column
        let before = &self.body[..self.body_cursor];
        let line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let col = before[line_start..].chars().count();
        // Next line starts after the newline
        let next_line_start = self.body_cursor + nl_offset + 1;
        if next_line_start > self.body.len() {
            self.body_cursor = self.body.len();
            return;
        }
        let next_line_end = self.body[next_line_start..].find('\n')
            .map(|p| next_line_start + p)
            .unwrap_or(self.body.len());
        let next_line = &self.body[next_line_start..next_line_end];
        let next_line_chars = next_line.chars().count();
        let target_col = col.min(next_line_chars);
        self.body_cursor = next_line.char_indices()
            .nth(target_col)
            .map(|(i, _)| next_line_start + i)
            .unwrap_or(next_line_end);
    }

    pub fn current_header_str(&self) -> &str {
        if self.header_edit_index < self.headers.len() {
            let (k, v) = &self.headers[self.header_edit_index];
            if self.header_edit_field == 0 { k } else { v }
        } else {
            ""
        }
    }

    /// Get the cursor column (char count) in the current header field.
    pub fn header_cursor_col(&self) -> usize {
        let s = self.current_header_str();
        s[..self.header_cursor.min(s.len())].chars().count()
    }

    pub fn insert_at_header(&mut self, text: &str) {
        if self.header_edit_index >= self.headers.len() {
            self.headers.push((String::new(), String::new()));
            self.header_cursor = 0;
        }
        let idx = self.header_edit_index;
        if idx < self.headers.len() {
            let field = if self.header_edit_field == 0 {
                &mut self.headers[idx].0
            } else {
                &mut self.headers[idx].1
            };
            let cursor = self.header_cursor.min(field.len());
            field.insert_str(cursor, text);
            self.header_cursor = cursor + text.len();
        }
    }

    pub fn backspace_header(&mut self) {
        if self.header_edit_index >= self.headers.len() { return; }
        let cursor = self.header_cursor;
        if cursor == 0 {
            if self.header_edit_field == 1 {
                self.header_edit_field = 0;
                self.header_cursor = self.headers[self.header_edit_index].0.len();
                return;
            }
            let (k, v) = &self.headers[self.header_edit_index];
            if k.is_empty() && v.is_empty() {
                self.delete_current_header();
            }
            return;
        }
        let idx = self.header_edit_index;
        let field = if self.header_edit_field == 0 {
            &mut self.headers[idx].0
        } else {
            &mut self.headers[idx].1
        };
        let c = field[..cursor].char_indices().rev().next()
            .map(|(i, _)| i).unwrap_or(0);
        field.drain(c..cursor);
        self.header_cursor = c;
    }

    pub fn move_header_left(&mut self) {
        if self.header_cursor == 0 {
            // At start of value? Jump to end of key
            if self.header_edit_field == 1 {
                self.header_edit_field = 0;
                if self.header_edit_index < self.headers.len() {
                    self.header_cursor = self.headers[self.header_edit_index].0.len();
                }
            }
            return;
        }
        let s = self.current_header_str();
        let cursor = self.header_cursor.min(s.len());
        self.header_cursor = s[..cursor].char_indices().rev().next()
            .map(|(i, _)| i).unwrap_or(0);
    }

    pub fn move_header_right(&mut self) {
        let s = self.current_header_str();
        let cursor = self.header_cursor.min(s.len());
        if cursor >= s.len() {
            // At end of key? Jump to start of value
            if self.header_edit_field == 0 {
                self.header_edit_field = 1;
                self.header_cursor = 0;
            }
            return;
        }
        self.header_cursor = s[cursor..].char_indices().nth(1)
            .map(|(i, _)| cursor + i).unwrap_or(s.len());
    }

    pub fn delete_current_header(&mut self) {
        if self.header_edit_index < self.headers.len() {
            self.headers.remove(self.header_edit_index);
            if self.header_edit_index >= self.headers.len() && self.header_edit_index > 0 {
                self.header_edit_index -= 1;
            }
            // Reset cursor to end of new current header
            if self.header_edit_index < self.headers.len() {
                let s = if self.header_edit_field == 0 {
                    &self.headers[self.header_edit_index].0
                } else {
                    &self.headers[self.header_edit_index].1
                };
                self.header_cursor = s.len();
            } else {
                self.header_cursor = 0;
            }
        }
    }

    /// Focus a header by index, placing cursor at end of value.
    pub fn focus_header(&mut self, index: usize) {
        if index < self.headers.len() {
            self.header_edit_index = index;
            self.header_edit_field = 1;
            self.header_cursor = self.headers[index].1.len();
        }
    }

    /// Add a new empty header and focus it.
    pub fn add_header(&mut self) {
        self.headers.push((String::new(), String::new()));
        self.header_edit_index = self.headers.len() - 1;
        self.header_edit_field = 0;
        self.header_cursor = 0;
    }

    pub fn status_color(&self) -> [f32; 4] {
        match self.response_status {
            Some(s) if s >= 200 && s < 300 => [0.4, 0.85, 0.5, 1.0],
            Some(s) if s >= 300 && s < 400 => [0.95, 0.75, 0.25, 1.0],
            Some(s) if s >= 400 => [0.95, 0.4, 0.4, 1.0],
            _ => [0.5, 0.5, 0.5, 1.0],
        }
    }

    pub fn response_line_count(&self) -> usize {
        if self.response_body.is_empty() {
            0
        } else {
            self.response_body.lines().count()
        }
    }

    // ── Search ──

    pub fn toggle_search(&mut self) {
        self.search_active = !self.search_active;
        if self.search_active {
            self.search_query.clear();
            self.search_cursor = 0;
            self.search_matches.clear();
            self.search_current = 0;
            self.focus_field = HttpField::Response;
        }
    }

    pub fn update_search(&mut self) {
        self.search_matches.clear();
        self.search_current = 0;
        if self.search_query.is_empty() || self.response_body.is_empty() {
            return;
        }
        let query_lower = self.search_query.to_lowercase();
        for (line_idx, line) in self.response_body.lines().enumerate() {
            let line_lower = line.to_lowercase();
            let mut start = 0;
            while let Some(pos) = line_lower[start..].find(&query_lower) {
                let col = line[..start + pos].chars().count();
                self.search_matches.push((line_idx, col));
                start += pos + query_lower.len();
            }
        }
    }

    pub fn search_insert(&mut self, text: &str) {
        self.search_query.insert_str(self.search_cursor, text);
        self.search_cursor += text.len();
        self.update_search();
        self.jump_to_current_match();
    }

    pub fn search_backspace(&mut self) {
        if self.search_cursor > 0 {
            let prev = self.search_query[..self.search_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
            self.search_query.drain(prev..self.search_cursor);
            self.search_cursor = prev;
            self.update_search();
            self.jump_to_current_match();
        }
    }

    pub fn search_next(&mut self) {
        if !self.search_matches.is_empty() {
            self.search_current = (self.search_current + 1) % self.search_matches.len();
            self.jump_to_current_match();
        }
    }

    pub fn search_prev(&mut self) {
        if !self.search_matches.is_empty() {
            self.search_current = if self.search_current == 0 {
                self.search_matches.len() - 1
            } else {
                self.search_current - 1
            };
            self.jump_to_current_match();
        }
    }

    fn jump_to_current_match(&mut self) {
        if let Some(&(line_idx, _)) = self.search_matches.get(self.search_current) {
            self.scroll_offset = line_idx.saturating_sub(3); // show match ~3 lines from top
        }
    }

    pub fn search_move_left(&mut self) {
        if self.search_cursor > 0 {
            self.search_cursor = self.search_query[..self.search_cursor]
                .char_indices().rev().next()
                .map(|(i, _)| i).unwrap_or(0);
        }
    }

    pub fn search_move_right(&mut self) {
        if self.search_cursor < self.search_query.len() {
            self.search_cursor = self.search_query[self.search_cursor..]
                .char_indices().nth(1)
                .map(|(i, _)| self.search_cursor + i)
                .unwrap_or(self.search_query.len());
        }
    }

    // ── Tree view ──

    pub fn toggle_response_view(&mut self) {
        self.response_view = match self.response_view {
            ResponseView::Raw => ResponseView::Tree,
            ResponseView::Tree => ResponseView::Raw,
        };
        self.tree_scroll = 0;
    }

    pub fn toggle_tree_node(&mut self, path: &str) {
        if self.tree_collapsed.contains(path) {
            self.tree_collapsed.remove(path);
        } else {
            self.tree_collapsed.insert(path.to_string());
        }
    }

    /// Build flattened tree lines from the response JSON.
    pub fn build_tree_lines(&self) -> Vec<TreeLine> {
        let mut lines = Vec::new();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.response_body) {
            Self::flatten_json(&value, 0, None, String::new(), &self.tree_collapsed, &mut lines);
        }
        lines
    }

    fn flatten_json(
        value: &serde_json::Value,
        depth: usize,
        key: Option<&str>,
        path: String,
        collapsed: &std::collections::HashSet<String>,
        lines: &mut Vec<TreeLine>,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                let is_collapsed = collapsed.contains(&path);
                let child_count = map.len();
                let preview = if is_collapsed {
                    // Show inline preview of collapsed object
                    let items: Vec<String> = map.iter().take(3)
                        .map(|(k, v)| format!("{}: {}", k, Self::value_preview(v)))
                        .collect();
                    let suffix = if child_count > 3 { ", ..." } else { "" };
                    format!("{{ {} {} }}", items.join(", "), suffix)
                } else {
                    format!("{{}} {} keys", child_count)
                };
                lines.push(TreeLine {
                    depth,
                    key: key.map(|s| s.to_string()),
                    preview,
                    is_expandable: true,
                    expanded: !is_collapsed,
                    path: path.clone(),
                    value_kind: TreeValueKind::Object,
                });
                if !is_collapsed {
                    for (k, v) in map {
                        let child_path = if path.is_empty() {
                            k.clone()
                        } else {
                            format!("{}.{}", path, k)
                        };
                        Self::flatten_json(v, depth + 1, Some(k), child_path, collapsed, lines);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                let is_collapsed = collapsed.contains(&path);
                let child_count = arr.len();
                let preview = if is_collapsed {
                    let items: Vec<String> = arr.iter().take(3)
                        .map(|v| Self::value_preview(v))
                        .collect();
                    let suffix = if child_count > 3 { ", ..." } else { "" };
                    format!("[{}{}]", items.join(", "), suffix)
                } else {
                    format!("[] {} items", child_count)
                };
                lines.push(TreeLine {
                    depth,
                    key: key.map(|s| s.to_string()),
                    preview,
                    is_expandable: true,
                    expanded: !is_collapsed,
                    path: path.clone(),
                    value_kind: TreeValueKind::Array,
                });
                if !is_collapsed {
                    for (i, v) in arr.iter().enumerate() {
                        let child_path = format!("{}[{}]", path, i);
                        let idx_key = format!("{}", i);
                        Self::flatten_json(v, depth + 1, Some(&idx_key), child_path, collapsed, lines);
                    }
                }
            }
            serde_json::Value::String(s) => {
                let display = if s.len() > 60 {
                    format!("\"{}...\"", &s[..57])
                } else {
                    format!("\"{}\"", s)
                };
                lines.push(TreeLine {
                    depth, key: key.map(|s| s.to_string()), preview: display,
                    is_expandable: false, expanded: false, path,
                    value_kind: TreeValueKind::String,
                });
            }
            serde_json::Value::Number(n) => {
                lines.push(TreeLine {
                    depth, key: key.map(|s| s.to_string()), preview: n.to_string(),
                    is_expandable: false, expanded: false, path,
                    value_kind: TreeValueKind::Number,
                });
            }
            serde_json::Value::Bool(b) => {
                lines.push(TreeLine {
                    depth, key: key.map(|s| s.to_string()), preview: b.to_string(),
                    is_expandable: false, expanded: false, path,
                    value_kind: TreeValueKind::Boolean,
                });
            }
            serde_json::Value::Null => {
                lines.push(TreeLine {
                    depth, key: key.map(|s| s.to_string()), preview: "null".to_string(),
                    is_expandable: false, expanded: false, path,
                    value_kind: TreeValueKind::Null,
                });
            }
        }
    }

    fn value_preview(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::String(s) => {
                if s.len() > 20 { format!("\"{}...\"", &s[..17]) } else { format!("\"{}\"", s) }
            }
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => "null".to_string(),
            serde_json::Value::Object(m) => format!("{{...}} {} keys", m.len()),
            serde_json::Value::Array(a) => format!("[...] {} items", a.len()),
        }
    }

    pub fn show_toast(&mut self, msg: &str) {
        self.toast_message = Some(msg.to_string());
        self.toast_time = Some(Instant::now());
    }

    pub fn toast_visible(&self) -> bool {
        self.toast_time.map(|t| t.elapsed().as_millis() < 1500).unwrap_or(false)
    }

    /// Generate a curl command from the current request state.
    pub fn to_curl(&self) -> String {
        let mut parts = vec![format!("curl '{}'", self.url)];
        if self.method != HttpMethod::GET {
            parts.push(format!("-X {}", self.method.as_str()));
        }
        for (k, v) in &self.headers {
            if !k.is_empty() {
                parts.push(format!("-H '{}: {}'", k, v));
            }
        }
        if !self.body.is_empty() {
            let escaped = self.body.replace('\'', "'\\''");
            parts.push(format!("-d '{}'", escaped));
        }
        parts.join(" \\\n  ")
    }

    /// Format response size for display.
    pub fn response_size_display(&self) -> String {
        let bytes = self.response_body.len();
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        }
    }
}

/// Simple shell-like tokenizer: handles single/double quotes and backslash escapes.
fn shell_tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(c) = chars.next() {
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                current.push(c);
            }
        } else if in_double {
            if c == '"' {
                in_double = false;
            } else if c == '\\' {
                if let Some(&next) = chars.peek() {
                    if matches!(next, '"' | '\\' | '$' | '`') {
                        current.push(chars.next().unwrap());
                    } else {
                        current.push(c);
                    }
                }
            } else {
                current.push(c);
            }
        } else if c == '\'' {
            in_single = true;
        } else if c == '"' {
            in_double = true;
        } else if c == '\\' {
            if let Some(next) = chars.next() {
                if next != '\n' {
                    current.push(next);
                }
            }
        } else if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
