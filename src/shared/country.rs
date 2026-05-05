use std::collections::HashMap;
use std::sync::LazyLock;

pub static COUNTRY_ID_TO_NAME: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
        HashMap::from([
            ("AO", "Angola"),
            ("AU", "Australia"),
            ("BF", "Burkina Faso"),
            ("BI", "Burundi"),
            ("BJ", "Benin"),
            ("BR", "Brazil"),
            ("BW", "Botswana"),
            ("CA", "Canada"),
            ("CD", "DR Congo"),
            ("CF", "Central African Republic"),
            ("CG", "Republic of the Congo"),
            ("CI", "Côte d'Ivoire"),
            ("CM", "Cameroon"),
            ("CN", "China"),
            ("CV", "Cape Verde"),
            ("DE", "Germany"),
            ("DJ", "Djibouti"),
            ("DZ", "Algeria"),
            ("EG", "Egypt"),
            ("EH", "Western Sahara"),
            ("ER", "Eritrea"),
            ("ET", "Ethiopia"),
            ("FR", "France"),
            ("GA", "Gabon"),
            ("GB", "United Kingdom"),
            ("GH", "Ghana"),
            ("GM", "Gambia"),
            ("GN", "Guinea"),
            ("GQ", "Equatorial Guinea"),
            ("GW", "Guinea-Bissau"),
            ("IN", "India"),
            ("JP", "Japan"),
            ("KE", "Kenya"),
            ("KM", "Comoros"),
            ("LR", "Liberia"),
            ("LS", "Lesotho"),
            ("LY", "Libya"),
            ("MA", "Morocco"),
            ("MG", "Madagascar"),
            ("ML", "Mali"),
            ("MR", "Mauritania"),
            ("MU", "Mauritius"),
            ("MW", "Malawi"),
            ("MZ", "Mozambique"),
            ("NA", "Namibia"),
            ("NE", "Niger"),
            ("NG", "Nigeria"),
            ("RW", "Rwanda"),
            ("SC", "Seychelles"),
            ("SD", "Sudan"),
            ("SL", "Sierra Leone"),
            ("SN", "Senegal"),
            ("SO", "Somalia"),
            ("SS", "South Sudan"),
            ("ST", "São Tomé and Príncipe"),
            ("SZ", "Eswatini"),
            ("TD", "Chad"),
            ("TG", "Togo"),
            ("TN", "Tunisia"),
            ("TZ", "Tanzania"),
            ("UG", "Uganda"),
            ("US", "United States"),
            ("ZA", "South Africa"),
            ("ZM", "Zambia"),
            ("ZW", "Zimbabwe"),
        ])
    });

pub static COUNTRY_NAME_TO_ID: LazyLock<HashMap<String, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    for (&id, &name) in COUNTRY_ID_TO_NAME.iter() {
        m.insert(name.to_lowercase(), id);
        for part in name.split_whitespace() {
            if part.len() > 2 {
                m.insert(part.to_lowercase(), id);
            }
        }
    }
    m.insert("nigeria".to_string(), "NG");
    m.insert("kenya".to_string(), "KE");
    m.insert("ghana".to_string(), "GH");
    m.insert("angola".to_string(), "AO");
    m.insert("south africa".to_string(), "ZA");
    m.insert("egypt".to_string(), "EG");
    m.insert("tanzania".to_string(), "TZ");
    m.insert("uganda".to_string(), "UG");
    m.insert("ethiopia".to_string(), "ET");
    m.insert("cameroon".to_string(), "CM");
    m.insert("madagascar".to_string(), "MG");
    m.insert("mozambique".to_string(), "MZ");
    m.insert("dr congo".to_string(), "CD");
    m.insert("ivory coast".to_string(), "CI");
    m.insert("congo".to_string(), "CG");
    m.insert("us".to_string(), "US");
    m.insert("usa".to_string(), "US");
    m.insert("uk".to_string(), "GB");
    m.insert("britain".to_string(), "GB");
    m
});

pub fn country_id_to_name(id: &str) -> String {
    COUNTRY_ID_TO_NAME
        .get(id)
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.to_string())
}

pub fn country_name_to_id(name: &str) -> Option<&'static str> {
    COUNTRY_NAME_TO_ID.get(&name.to_lowercase()).copied()
}
