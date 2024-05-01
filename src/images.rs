use image::GenericImageView;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{out::Out, types};

#[derive(Debug)]
struct Response {
    images: types::Images,
    write_files: Vec<(PathBuf, Vec<u8>)>,
}

#[inline]
fn make_image_path_for_width<const W: u32>(path: &Path) -> PathBuf {
    let mut image_path = path.to_owned();
    let file_stem = image_path.file_stem().unwrap();
    let extension = image_path.extension().unwrap();
    let mut file_name = file_stem.to_owned();
    file_name.push(format!("-{}.", W));
    file_name.push(extension);
    image_path.set_file_name(file_name);
    image_path
}

fn encode_image(
    image: &image::DynamicImage,
    format: image::ImageFormat,
) -> anyhow::Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());

    match format {
        image::ImageFormat::Png => {
            let encoder = image::codecs::png::PngEncoder::new_with_quality(
                &mut buf,
                image::codecs::png::CompressionType::Best,
                image::codecs::png::FilterType::NoFilter,
            );
            image.write_with_encoder(encoder)?;
        }
        _ => {
            image.write_to(&mut buf, format)?;
        }
    }

    Ok(buf.into_inner())
}

fn extract_image(out_file: PathBuf, image_data: Vec<u8>) -> anyhow::Result<Response> {
    let mut images = types::Images {
        original: out_file.clone(),
        original_width: None,
        x_1536: None,
        x_768: None,
    };

    let format = match image::ImageFormat::from_path(&out_file) {
        Ok(format) => format,
        Err(_) => {
            return anyhow::Ok(Response {
                images,
                write_files: vec![(out_file, image_data)],
            });
        }
    };

    let image = {
        let mut reader = image::io::Reader::new(Cursor::new(&image_data));
        reader.set_format(format);
        reader.decode()?
    };
    let (width, height) = image.dimensions();

    images.original_width = Some(width);

    let (try_reencode, target_format) = match format {
        image::ImageFormat::Jpeg => (false, image::ImageFormat::Jpeg),
        image::ImageFormat::Png => (true, image::ImageFormat::Png),
        image::ImageFormat::WebP => (true, image::ImageFormat::WebP),
        _ => (true, image::ImageFormat::Png),
    };

    let full = if format != target_format || try_reencode {
        let reencoded = encode_image(&image, target_format)?;
        if format != target_format || reencoded.len() < image_data.len() {
            reencoded
        } else {
            image_data
        }
    } else {
        image_data
    };

    let mut write_files = vec![];
    if width > 1536 {
        let out_file = make_image_path_for_width::<1536>(&out_file);

        let image = image.resize(1536, height, image::imageops::FilterType::Lanczos3);
        let result = encode_image(&image, format)?;
        if result.len() < full.len() {
            images.x_1536 = Some(out_file.clone());
            write_files.push((out_file, result));
        }
    }

    if width > 768 {
        let out_file = make_image_path_for_width::<768>(&out_file);

        let image = image.resize(768, height, image::imageops::FilterType::Lanczos3);
        let result = encode_image(&image, format)?;
        if result.len() < full.len() {
            images.x_768 = Some(out_file.clone());
            write_files.push((out_file, result));
        }
    }
    write_files.push((out_file, full));

    anyhow::Ok(Response {
        images,
        write_files,
    })
}

pub fn extract_images<'a>(
    out: &Out,
    entries: &[types::EntryMeta],
    parsed_entries: &[Vec<jotdown::Event<'a>>],
) -> anyhow::Result<Vec<HashMap<String, types::Images>>> {
    let (tx, rx) = std::sync::mpsc::channel::<(usize, String, anyhow::Result<Response>)>();

    let mut images = (0..entries.len())
        .map(|_| HashMap::new())
        .collect::<Vec<_>>();

    // Read and write image data serially, process concurrently. Uses quite some memory but keeps
    // i/o fast
    std::thread::scope(|std_s| {
        // mutex to coordinate only reading or writing at any one time
        let mutex = Arc::new(Mutex::new(()));

        let t = {
            let mutex = mutex.clone();
            std_s.spawn(move || {
                for (idx, link, response) in rx {
                    let response = response?;
                    images[idx].insert(link, response.images);
                    for (path, content) in response.write_files {
                        let m = mutex.lock().unwrap();
                        out.update_file(&mut &*content, path)?;
                        drop(m);
                    }
                }

                return anyhow::Ok(images);
            })
        };

        rayon::scope(move |s| {
            let mut links = HashSet::<&'a str>::new();
            for (idx, (entry, parsed_entry)) in entries.iter().zip(parsed_entries).enumerate() {
                links.clear();

                for event in parsed_entry {
                    if let jotdown::Event::Start(jotdown::Container::Image(link, _), _) = event {
                        links.insert(link.as_ref());
                    }
                }

                for image_link in links.drain() {
                    let in_file = entry.asset_dir.join(image_link);
                    let out_file = entry.out_asset_dir.join(image_link);
                    let m = mutex.lock().unwrap();
                    let image_data = fs::read(&in_file)?;
                    drop(m);

                    let tx = tx.clone();
                    // this provides no backpresure. if processing is much slower than reading from
                    // disk, we can easily exhaust memory
                    s.spawn(move |_| {
                        tx.send((
                            idx,
                            image_link.to_owned(),
                            extract_image(out_file, image_data),
                        ))
                        .unwrap();
                    });
                }
            }

            anyhow::Ok(())
        })?;

        anyhow::Ok(t.join().unwrap()?)
    })
}
