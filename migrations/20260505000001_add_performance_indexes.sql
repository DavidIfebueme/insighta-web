CREATE INDEX idx_profiles_country_gender_agegroup ON profiles (country_id, gender, age_group);
CREATE INDEX idx_profiles_age ON profiles (age);
CREATE INDEX idx_profiles_created_at ON profiles (created_at DESC);
