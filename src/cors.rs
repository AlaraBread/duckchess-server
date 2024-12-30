use rocket::http::Method;
use rocket_cors::{AllowedHeaders, AllowedOrigins, Cors, CorsOptions};

pub fn stage() -> Cors {
	let allowed_origins = AllowedOrigins::some_exact(&["http://localhost:3000"]);

	return CorsOptions {
		allowed_origins,
		allowed_methods: vec![Method::Get, Method::Post]
			.into_iter()
			.map(From::from)
			.collect(),
		allowed_headers: AllowedHeaders::some(&["Authorization", "Accept"]),
		allow_credentials: true,
		..Default::default()
	}
	.to_cors()
	.unwrap();
}
