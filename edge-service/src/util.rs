use ws::stream::DuplexStream;

pub async fn close_socket(socket: &mut DuplexStream, message: String) {
	let close_frame = ws::frame::CloseFrame {
		code: ws::frame::CloseCode::Normal,
		reason: message.into(),
	};
	let _ = socket.close(Some(close_frame)).await;
}

pub async fn conditional_future<'a, T, F: Future<Output = T>>(future: Option<F>) -> Option<T> {
	match future {
		Some(future) => Some(future.await),
		None => None,
	}
}
