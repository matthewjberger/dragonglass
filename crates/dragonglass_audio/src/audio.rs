use std::io::BufReader;
use std::thread;
use std::time::Duration;

#[derive(Default)]
pub struct Audio;

impl Audio {
    pub fn play_music(path: &str) {
        let path = path.to_string();
        thread::spawn(move || {
            let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
            let sink = rodio::Sink::try_new(&handle).unwrap();

            let file = std::fs::File::open(path).unwrap();
            sink.append(rodio::Decoder::new_looped(BufReader::new(file)).unwrap());

            sink.sleep_until_end();
        });
    }

    pub fn play_spatial_sound(path: String) {
        thread::spawn(move || {
            let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
            let sink = rodio::SpatialSink::try_new(
                &handle,
                [-10.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0],
            )
            .unwrap();

            let file = std::fs::File::open(path).unwrap();
            let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
            sink.append(source);

            // A sound emitter playing the music starting at the left gradually moves to the right
            // eventually passing through the listener, then it continues on to the right for a distance
            // until it stops and begins traveling to the left, it will eventually pass through the
            // listener again.
            // This is repeated 5 times.
            for _ in 0..5 {
                for i in 1..1001 {
                    thread::sleep(Duration::from_millis(5));
                    sink.set_emitter_position([(i - 500) as f32 / 50.0, 0.0, 0.0]);
                }
                for i in 1..1001 {
                    thread::sleep(Duration::from_millis(5));
                    sink.set_emitter_position([-(i - 500) as f32 / 50.0, 0.0, 0.0]);
                }
            }
            sink.sleep_until_end();
        });
    }
}

#[allow(dead_code)]
fn spatial() {
    let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
    let sink = rodio::SpatialSink::try_new(
        &handle,
        [-10.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
    )
    .unwrap();

    let file = std::fs::File::open("assets/sounds/music.mp3").unwrap();
    let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
    sink.append(source);

    // A sound emitter playing the music starting at the left gradually moves to the right
    // eventually passing through the listener, then it continues on to the right for a distance
    // until it stops and begins traveling to the left, it will eventually pass through the
    // listener again.
    // This is repeated 5 times.
    for _ in 0..5 {
        for i in 1..1001 {
            thread::sleep(Duration::from_millis(5));
            sink.set_emitter_position([(i - 500) as f32 / 50.0, 0.0, 0.0]);
        }
        for i in 1..1001 {
            thread::sleep(Duration::from_millis(5));
            sink.set_emitter_position([-(i - 500) as f32 / 50.0, 0.0, 0.0]);
        }
    }
    sink.sleep_until_end();
}
