use rand::Rng;
use ws::stream::DuplexStream;

pub async fn close_socket(socket: &mut DuplexStream, message: String) {
	let close_frame = ws::frame::CloseFrame {
		code: ws::frame::CloseCode::Normal,
		reason: message.into(),
	};
	let _ = socket.close(Some(close_frame)).await;
}

pub fn randomly_permute_2<T>(choices: (T, T)) -> (T, T) {
	match rand::rng().random() {
		true => (choices.0, choices.1),
		false => (choices.1, choices.0),
	}
}
