use rocket::http::SameSite;
use rocket::serde::{
	Deserialize,
	de::{Error, Unexpected},
};

#[derive(Deserialize, Debug, Clone)]
#[serde(crate = "rocket::serde")]
pub struct CustomConfig {
	#[serde(default = "get_default_cors_allowed_origins")]
	pub cors_allowed_origins: Vec<String>,
	#[serde(default = "get_default_allow_all_origins")]
	pub cors_allow_all_origins: bool,
	#[serde(default = "get_default_cookies_same_site")]
	pub cookies_same_site: SameSiteConfig,
}

#[derive(Debug, Clone)]
pub struct SameSiteConfig(SameSite);

impl<'de> Deserialize<'de> for SameSiteConfig {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: rocket::serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		match s.as_str() {
			"Lax" => Ok(SameSiteConfig(SameSite::Lax)),
			"Strict" => Ok(SameSiteConfig(SameSite::Strict)),
			"None" => Ok(SameSiteConfig(SameSite::None)),
			_ => Err(D::Error::invalid_value(
				Unexpected::Str(&s),
				&"Lax, Strict, or None",
			)),
		}
	}
}

impl From<&SameSiteConfig> for SameSite {
	fn from(config: &SameSiteConfig) -> Self {
		config.0
	}
}

fn get_default_cors_allowed_origins() -> Vec<String> {
	vec![]
}

fn get_default_allow_all_origins() -> bool {
	false
}

fn get_default_cookies_same_site() -> SameSiteConfig {
	SameSiteConfig(SameSite::Lax)
}
