//! Draw what Tailorbird shows for a conversation, on this computer, with the renderer the program
//! itself uses: a native test then compares its screen captures with these, pixel for pixel.
//! The conversation is driven through the same calls the program makes, from a script, one step a line:
//!
//!   ready | loading [<checked> <of>] | gone | not-started      what the model service is
//!   type <words>                                  into the field
//!   enter | stop | new | took                     the keys (new: Alt+C), and "the request was carried"
//!   chunk <words> | done | failed                 from the service, for the reply being waited for
//!   notes                                          PCP OS lets this launch make notes
//!   folder <name>                                  the folder PCP OS said this launch builds into
//!   say <words>                                    the notice line, as the program sets it after a note
//!   hide                                           the Work panel leaves the conversation (after a note)
//!   shot <name>                                    write <name>.ppm of the screen as it now is
//!
//! Usage: talk_frames <script> <output directory> <width> <height>
//! These are pictures of the renderer's output, not of anything a model or PCP OS did.
use std::{env, fs, io::Write, path::Path};
use tailorbird_ui::{frame::Frame, palette::GROUND, talk::{Incoming, Service}, view::paint_frame, Action, Text, Ui};

fn main() {
    let args: Vec<String> = env::args().collect();
    let (script, out) = (fs::read_to_string(&args[1]).expect("script"), Path::new(&args[2]));
    let (width, height): (usize, usize) = (args[3].parse().unwrap(), args[4].parse().unwrap());
    fs::create_dir_all(out).unwrap();
    let mut ui = Ui::new();
    let mut ever_ready = false;
    let mut waited: Option<(u64, u64)> = None;
    for line in script.lines().filter(|l| !l.trim().is_empty()) {
        let (step, rest) = line.split_once(' ').unwrap_or((line, ""));
        match step {
            "ready" => {
                ever_ready = true;
                ui.model_ready()
            }
            // `loading`, or `loading <bytes checked> <of bytes>` as the service said it.
            "loading" => {
                let mut numbers = rest.split_whitespace().map(|n| n.parse::<u64>().expect("a number of bytes"));
                ui.talk.loading(numbers.next().zip(numbers.next()));
            }
            "not-started" => ui.talk.service_is(Service::Failed),
            "gone" => ui.talk.service_is(if ever_ready { Service::Stopped } else { Service::Failed }),
            "notes" => ui.notes = true,
            // The folder PCP OS said this launch builds into: new notes go to its first project.
            "folder" => ui.active = tailorbird_ui::project_location::BuildFolder::new(rest).ok().and_then(tailorbird_ui::note::default_project),
            "type" => {
                for b in rest.bytes() {
                    assert!(ui.input.push_ascii(b), "the field is full");
                }
            }
            "clear" => ui.input = Text::new(),
            "enter" => {
                let _ = ui.intent(Action::Submit);
                if let Some(tailorbird_ui::talk::Request::Say(message)) = ui.for_service() {
                    waited = Some((message.conversation, message.reply));
                }
            }
            "stop" => {
                let _ = ui.intent(Action::Stop);
            }
            "took" => ui.service_took(),
            "new" => {
                let _ = ui.new_conversation();
                // Whatever it queued for the service is taken as carried.
                while ui.for_service().is_some() {
                    ui.service_took();
                }
            }
            "chunk" => {
                let (conversation, reply) = waited.expect("no reply is waited for");
                ui.talk.receive(Incoming::Chunk { conversation, reply, text: rest });
            }
            "reading" => {
                let (conversation, reply) = waited.expect("no reply is waited for");
                let (read, of) = rest.split_once(' ').expect("reading READ OF");
                ui.talk.receive(Incoming::Reading { conversation, reply, read: read.parse().unwrap(), of: of.parse().unwrap() });
            }
            "done" => {
                let (conversation, reply) = waited.expect("no reply is waited for");
                ui.talk.receive(Incoming::Done { conversation, reply });
            }
            "failed" => {
                let (conversation, reply) = waited.expect("no reply is waited for");
                ui.talk.receive(Incoming::Failed { conversation, reply });
            }
            "say" => ui.notice = Text::display(rest),
            "hide" => ui.talk_shown = false,
            "shot" => {
                let mut pixels = vec![GROUND; width * height];
                paint_frame(&mut Frame::new(&mut pixels, width, height, width, GROUND, ui.compact_view), &ui);
                let mut file = fs::File::create(out.join(format!("{rest}.ppm"))).unwrap();
                write!(file, "P6\n{width} {height}\n255\n").unwrap();
                let rgb: Vec<u8> = pixels.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
                file.write_all(&rgb).unwrap();
            }
            other => panic!("unknown step {other:?}"),
        }
    }
}
