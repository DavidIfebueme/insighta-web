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
    let normalized = lower
        .replace(['–', '—', '–'], "-")
        .replace(['/', '\\'], "-");
    let tokens: Vec<&str> = normalized.split_whitespace().collect();

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

        if let Some(range) = try_age_range(token) {
            parsed.min_age = Some(range.0);
            parsed.max_age = Some(range.1);
            matched_any = true;
        } else if token == "male"
            || token == "males"
            || token == "men"
            || token == "man"
            || token == "boys"
            || token == "boy"
        {
            saw_male = true;
            parsed.gender = Some("male".to_string());
            matched_any = true;
        } else if token == "female"
            || token == "females"
            || token == "women"
            || token == "woman"
            || token == "girls"
            || token == "girl"
        {
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
        } else if token == "teenager" || token == "teenagers" || token == "teens" {
            parsed.age_group = Some("teenager".to_string());
            matched_any = true;
        } else if token == "child" || token == "children" || token == "kids" || token == "kid" {
            parsed.age_group = Some("child".to_string());
            matched_any = true;
        } else if token == "senior" || token == "seniors" || token == "elderly" {
            parsed.age_group = Some("senior".to_string());
            matched_any = true;
        } else if (token == "above"
            || token == "over"
            || token == "older"
            || token == "olderthan"
            || token == "above_the_age_of"
            || token == "above_the_age")
            && i + 1 < tokens.len()
        {
            if let Some(age) = parse_age_token(tokens[i + 1]) {
                parsed.min_age = Some(age);
                matched_any = true;
                i += 1;
            }
        } else if (token == "below"
            || token == "under"
            || token == "younger"
            || token == "youngerthan"
            || token == "below_the_age_of"
            || token == "below_the_age")
            && i + 1 < tokens.len()
        {
            if let Some(age) = parse_age_token(tokens[i + 1]) {
                parsed.max_age = Some(age);
                matched_any = true;
                i += 1;
            }
        } else if (token == "between" || token == "aged" || token == "ages" || token == "age")
            && i + 1 < tokens.len()
        {
            if let Some(range) = try_age_range(tokens[i + 1]) {
                parsed.min_age = Some(range.0);
                parsed.max_age = Some(range.1);
                matched_any = true;
                i += 1;
            } else if let Some(age) = parse_age_token(tokens[i + 1]) {
                if token == "aged" || token == "age" || token == "ages" {
                    parsed.min_age = Some(age);
                    parsed.max_age = Some(age);
                    matched_any = true;
                    i += 1;
                } else if i + 2 < tokens.len() && (tokens[i + 2] == "and" || tokens[i + 2] == "-") {
                    if let Some(age2) = parse_age_token(tokens.get(i + 3).copied().unwrap_or("")) {
                        parsed.min_age = Some(age);
                        parsed.max_age = Some(age2);
                        matched_any = true;
                        i += 3;
                    }
                }
            }
        } else if token == "from" || token == "in" || token == "living" || token == "of" {
            if token == "living" && i + 1 < tokens.len() && tokens[i + 1] == "in" {
                i += 1;
            }
            if i + 1 < tokens.len() {
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
            }
        } else if token != "and"
            && token != "the"
            && token != "of"
            && token != "with"
            && token != "who"
            && token != "are"
            && token != "is"
            && token != "to"
            && token != "aged"
            && token != "ages"
            && token != "between"
            && token != "years"
            && token != "year"
            && token != "old"
            && token != "than"
            && token != "living"
            && token != "from"
            && token != "in"
            && token != "people"
            && token != "persons"
            && token != "person"
            && token != "their"
            && token != "show"
            && token != "find"
            && token != "get"
            && token != "list"
            && token != "all"
            && token != "me"
            && token != "a"
            && token != "an"
            && parsed.country_id.is_none()
        {
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

        i += 1;
    }

    if saw_male && saw_female {
        parsed.gender = None;
    }

    if matched_any { Some(parsed) } else { None }
}

fn try_age_range(token: &str) -> Option<(i32, i32)> {
    let parts: Vec<&str> = token.split('-').collect();
    if parts.len() == 2 {
        let lo = parts[0].parse::<i32>().ok()?;
        let hi = parts[1]
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse::<i32>()
            .ok()?;
        if lo > 0 && hi > 0 && lo <= hi {
            return Some((lo, hi));
        }
    }
    None
}

fn parse_age_token(token: &str) -> Option<i32> {
    token
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .ok()
}

fn is_keyword(t: &str) -> bool {
    matches!(
        t,
        "male"
            | "males"
            | "men"
            | "man"
            | "boys"
            | "boy"
            | "female"
            | "females"
            | "women"
            | "woman"
            | "girls"
            | "girl"
            | "young"
            | "adult"
            | "adults"
            | "teenager"
            | "teenagers"
            | "teens"
            | "child"
            | "children"
            | "kids"
            | "kid"
            | "senior"
            | "seniors"
            | "elderly"
            | "above"
            | "over"
            | "older"
            | "olderthan"
            | "below"
            | "under"
            | "younger"
            | "youngerthan"
            | "between"
            | "aged"
            | "ages"
            | "age"
            | "from"
            | "in"
            | "and"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nigerian_females_age_range() {
        let q = parse_natural_language("nigerian females between the ages of 20-74");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.country_id, Some("NG".to_string()));
        assert_eq!(p.gender, Some("female".to_string()));
        assert_eq!(p.min_age, Some(20));
        assert_eq!(p.max_age, Some(74));
    }

    #[test]
    fn test_women_aged_range_living_in() {
        let q = parse_natural_language("women aged 20-45 living in Nigeria");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.gender, Some("female".to_string()));
        assert_eq!(p.min_age, Some(20));
        assert_eq!(p.max_age, Some(45));
        assert_eq!(p.country_id, Some("NG".to_string()));
    }

    #[test]
    fn test_males_from_nigeria_over_30() {
        let q = parse_natural_language("males from Nigeria over 30");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.gender, Some("male".to_string()));
        assert_eq!(p.country_id, Some("NG".to_string()));
        assert_eq!(p.min_age, Some(30));
    }

    #[test]
    fn test_between_25_and_50() {
        let q = parse_natural_language("females between 25 and 50 in Kenya");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.gender, Some("female".to_string()));
        assert_eq!(p.min_age, Some(25));
        assert_eq!(p.max_age, Some(50));
        assert_eq!(p.country_id, Some("KE".to_string()));
    }

    #[test]
    fn test_aged_single_value() {
        let q = parse_natural_language("males aged 30 from Ghana");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.gender, Some("male".to_string()));
        assert_eq!(p.min_age, Some(30));
        assert_eq!(p.max_age, Some(30));
        assert_eq!(p.country_id, Some("GH".to_string()));
    }

    #[test]
    fn test_young_adults_south_africa() {
        let q = parse_natural_language("young adults in South Africa");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.min_age, Some(16));
        assert_eq!(p.max_age, Some(24));
        assert_eq!(p.age_group, Some("adult".to_string()));
        assert_eq!(p.country_id, Some("ZA".to_string()));
    }

    #[test]
    fn test_elderly_women_rwanda() {
        let q = parse_natural_language("elderly women from Rwanda");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.gender, Some("female".to_string()));
        assert_eq!(p.age_group, Some("senior".to_string()));
        assert_eq!(p.country_id, Some("RW".to_string()));
    }

    #[test]
    fn test_teens_nigeria() {
        let q = parse_natural_language("teens in Nigeria");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.age_group, Some("teenager".to_string()));
        assert_eq!(p.country_id, Some("NG".to_string()));
    }

    #[test]
    fn test_em_dash_range() {
        let q = parse_natural_language("women aged 20—45 from Nigeria");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.min_age, Some(20));
        assert_eq!(p.max_age, Some(45));
        assert_eq!(p.country_id, Some("NG".to_string()));
    }

    #[test]
    fn test_under_18_boys_kenya() {
        let q = parse_natural_language("boys under 18 in Kenya");
        assert!(q.is_some());
        let p = q.unwrap();
        assert_eq!(p.gender, Some("male".to_string()));
        assert_eq!(p.max_age, Some(18));
        assert_eq!(p.country_id, Some("KE".to_string()));
    }
}
