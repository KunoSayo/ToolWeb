//! Run with
//!
//! ```not_rust
//! cd examples && cargo run -p example-stream-to-file
//! ```

use std::io;

use axum::{
    body::Bytes,
    BoxError,
    extract::{BodyStream, Multipart, Path},
    http::StatusCode,
    response::{Html, Redirect},
};
use axum::http::Request;
use futures::{Stream, TryStreamExt};
use tokio::{fs::File, io::BufWriter};
use tracing::log;

use tokio_util::io::StreamReader;

use crate::UPLOADS_DIRECTORY;

// Handler that streams the request body to a file.
//
// POST'ing to `/file/foo.txt` will create a file called `foo.txt`.
pub async fn save_request_body(
    Path(file_name): Path<String>,
    body: BodyStream,
) -> Result<(), (StatusCode, String)> {
    stream_to_file(&file_name, body).await
}

// Handler that returns HTML for a multipart form.
pub async fn show_form<B>(arg: Request<B>) -> Html<&'static str> {
    Html(include_str!("upload.html"))
}

// Handler that accepts a multipart form upload and streams each field to a file.
pub async fn accept_form(mut multipart: Multipart) -> Result<Redirect, (StatusCode, String)> {
    while let Some(field) = multipart.next_field().await.unwrap() {
        let file_name = if let Some(file_name) = field.file_name() {
            file_name.to_owned()
        } else {
            continue;
        };
        tracing::info!("Accept file {}", file_name);


        stream_to_file(&file_name, field).await?;
    }

    Ok(Redirect::to("/upload"))
}

// Save a `Stream` to a file
async fn stream_to_file<S, E>(path: &str, stream: S) -> Result<(), (StatusCode, String)>
    where
        S: Stream<Item=Result<Bytes, E>>,
        E: Into<BoxError>,
{
    if !path_is_valid(path) {
        return Err((StatusCode::BAD_REQUEST, "Invalid path".to_owned()));
    }

    async {
        // Convert the stream into an `AsyncRead`.
        let body_with_io_error = stream.map_err(|err| {
            let err = err.into();
            log::error!("stream error for {:?}", err);
            io::Error::new(io::ErrorKind::Other, err)
        });
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // Create the file. `File` implements `AsyncWrite`.
        let path = std::path::Path::new(UPLOADS_DIRECTORY).join(path);
        let mut file = BufWriter::new(File::create(path).await?);

        // Copy the body into the file.
        tokio::io::copy(&mut body_reader, &mut file).await?;

        Ok::<_, io::Error>(())
    }
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

// to prevent directory traversal attacks we ensure the path consists of exactly one normal
// component
fn path_is_valid(path: &str) -> bool {
    let path = std::path::Path::new(path);
    let mut components = path.components().peekable();

    if let Some(first) = components.peek() {
        if !matches!(first, std::path::Component::Normal(_)) {
            return false;
        }
    }

    components.count() == 1
}