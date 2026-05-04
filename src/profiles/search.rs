use crate::shared::country::country_name_to_id;

#[derive(Debug, Default)]
pub struct ParsedQuery {
    pub gender: Option<String>,
    pub age_group: Option<String>,
    pub country_id: Option<String>,
    pub min_age: Option<i32>,
    pub max_age: Option<i32>,
}

pub fn parse_natural_language(query: &str) -> Option<ParsedQuery> {
    let lower = query.to_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();

    if tokens.is_empty() {
        return None;
    }

    let mut parsed = ParsedQuery::default();
    let mut matched_any = false;
    let mut saw_male = false;
    let mut saw_female = false;

    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];

        if token == "male" || token == "males" {
            saw_male = true;
            parsed.gender = Some("male".to_string());
            matched_any = true;
        } else if token == "female" || token == "females" {
            saw_female = true;
            parsed.gender = Some("female".to_string());
            matched_any = true;
        } else if token == "young" {
            parsed.min_age = Some(16);
            parsed.max_age = Some(24);
            matched_any = true;
        } else if token == "adult" || token == "adults" {
            parsed.age_group = Some("adult".to_string());
            matched_any = true;
        } else if token == "teenager" || token == "teenagers" {
            parsed.age_group = Some("teenager".to_string());
            matched_any = true;
        } else if token == "child" || token == "children" {
            parsed.age_group = Some("child".to_string());
            matched_any = true;
        } else if token == "senior" || token == "seniors" {
            parsed.age_group = Some("senior".to_string());
            matched_any = true;
        } else if (token == "above" || token == "over" || token == "older") && i + 1 < tokens.len() {
            if let Some(age) = parse_age_token(tokens[i + 1]) {
                parsed.min_age = Some(age);
                matched_any = true;
                i += 1;
            }
        } else if (token == "below" || token == "under" || token == "younger") && i + 1 < tokens.len() {
            if let Some(age) = parse_age_token(tokens[i + 1]) {
                parsed.max_age = Some(age);
                matched_any = true;
                i += 1;
            }
        } else if token == "from" && i + 1 < tokens.len() {
            let mut country_tokens = Vec::new();
            let mut j = i + 1;
            while j < tokens.len() {
                let t = tokens[j];
                if is_keyword(t) {
                    break;
                }
                country_tokens.push(t);
                j += 1;
            }
            if !country_tokens.is_empty() {
                let country_name = country_tokens.join(" ");
                if let Some(id) = country_name_to_id(&country_name) {
                    parsed.country_id = Some(id.to_string());
                    matched_any = true;
                    i = j - 1;
                }
            }
        } else if token != "and" && token != "people" && token != "persons" && token != "person" && token != "the" {
            if parsed.country_id.is_none() {
                let mut country_tokens = vec![token];
                let mut j = i + 1;
                while j < tokens.len() {
                    let t = tokens[j];
                    if is_keyword(t) {
                        break;
                    }
                    country_tokens.push(t);
                    j += 1;
                }
                let country_name = country_tokens.join(" ");
                if let Some(id) = country_name_to_id(&country_name) {
                    parsed.country_id = Some(id.to_string());
                    matched_any = true;
                    i = j - 1;
                }
            }
        }

        i += 1;
    }

    if saw_male && saw_female {
        parsed.gender = None;
    }

    if matched_any {
        Some(parsed)
    } else {
        None
    }
}

fn parse_age_token(token: &str) -> Option<i32> {
    token.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()
}

fn is_keyword(t: &str) -> bool {
    matches!(
        t,
        "male" | "males"
            | "female" | "females"
            | "young" | "adult" | "adults"
            | "teenager" | "teenagers"
            | "child" | "children"
            | "senior" | "seniors"
            | "above" | "over" | "older"
            | "below" | "under" | "younger"
            | "from" | "and"
    )
}
