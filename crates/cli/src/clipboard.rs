use std::{io::BufWriter, path::Path};

use anyhow::{Context, Result, bail};
use arboard::{Clipboard, ImageData};

pub enum Content {
    Text(String),
    Image(tempfile::NamedTempFile),
}

pub fn directory() -> Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("beam-clipboard-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    Ok(builder.tempdir()?)
}

pub fn read(directory: &Path) -> Result<Content> {
    let mut clipboard = Clipboard::new().context(
        "Clipboard unavailable. Use terminal text paste or select a file; a local desktop session is required for images",
    )?;
    match clipboard.get_image() {
        Ok(image) => encode_image(image, directory).map(Content::Image),
        Err(arboard::Error::ContentNotAvailable) => {
            let text = clipboard
                .get_text()
                .context("The clipboard has no supported text or image")?;
            if text.is_empty() {
                bail!("The clipboard is empty");
            }
            Ok(Content::Text(text))
        }
        Err(error) => Err(error).context("Could not read the clipboard image"),
    }
}

fn encode_image(image: ImageData<'_>, directory: &Path) -> Result<tempfile::NamedTempFile> {
    let bytes = image
        .width
        .checked_mul(image.height)
        .and_then(|n| n.checked_mul(4));
    if image.width == 0
        || image.height == 0
        || bytes != Some(image.bytes.len())
        || image.bytes.len() > 128 * 1024 * 1024
    {
        bail!("Clipboard image is invalid or exceeds 128 MiB of decoded pixels");
    }
    let mut file = tempfile::Builder::new()
        .prefix("pasted-image-")
        .suffix(".png")
        .tempfile_in(directory)?;
    {
        let mut encoder = png::Encoder::new(
            BufWriter::new(file.as_file_mut()),
            image.width.try_into()?,
            image.height.try_into()?,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image.bytes)?;
        writer.finish()?;
    }
    file.as_file().sync_all()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn image_source_is_valid_png_and_deleted_with_its_owner() {
        let dir = tempfile::tempdir().unwrap();
        let file = encode_image(
            ImageData {
                width: 1,
                height: 1,
                bytes: Cow::Owned(vec![10, 20, 30, 255]),
            },
            dir.path(),
        )
        .unwrap();
        let path = file.path().to_owned();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        assert_eq!(pixels, [10, 20, 30, 255]);
        drop(file);
        assert!(!path.exists());
    }

    #[test]
    fn invalid_dimensions_do_not_create_a_source() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            encode_image(
                ImageData {
                    width: usize::MAX,
                    height: 2,
                    bytes: Cow::Borrowed(&[])
                },
                dir.path()
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
