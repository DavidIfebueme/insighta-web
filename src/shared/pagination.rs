use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PaginationLinks {
    #[serde(rename = "self")]
    pub self_link: Option<String>,
    pub next: Option<String>,
    pub prev: Option<String>,
}

pub fn build_links(base_path: &str, page: i64, limit: i64, total: i64, query_string: &str) -> PaginationLinks {
    let total_pages = ((total as f64) / (limit as f64)).ceil() as i64;

    let qs = if query_string.is_empty() {
        String::new()
    } else {
        format!("&{}", query_string)
    };

    let self_link = format!("{}?page={}&limit={}{}", base_path, page, limit, qs);
    let next = if page < total_pages {
        Some(format!("{}?page={}&limit={}{}", base_path, page + 1, limit, qs))
    } else {
        None
    };
    let prev = if page > 1 {
        Some(format!("{}?page={}&limit={}{}", base_path, page - 1, limit, qs))
    } else {
        None
    };

    PaginationLinks {
        self_link: Some(self_link),
        next,
        prev,
    }
}

pub fn total_pages(total: i64, limit: i64) -> i64 {
    if limit == 0 {
        return 0;
    }
    ((total as f64) / (limit as f64)).ceil() as i64
}
