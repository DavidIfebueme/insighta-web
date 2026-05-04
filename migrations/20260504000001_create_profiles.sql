CREATE TABLE profiles (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    gender VARCHAR(50) NOT NULL,
    gender_probability DOUBLE PRECISION NOT NULL,
    sample_size INTEGER NOT NULL,
    age INTEGER NOT NULL,
    age_group VARCHAR(20) NOT NULL,
    country_id VARCHAR(10) NOT NULL,
    country_probability DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_profiles_name_lower ON profiles (LOWER(name));
