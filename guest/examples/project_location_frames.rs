//! Render the longest accepted target for an independent literal/font check.
//! These are disclosed UI sample frames, not filesystem or kernel execution.
use std::{env, fs, io::Write, path::Path};
use tailorbird_ui::{frame::Frame, live_view, live_work::{Work,Update,ChangeState,PREVIEW_PIXELS},
                    palette::GROUND, project_location::{BuildFolder,ProjectLocation}};

fn main() {
    let root=env::args().nth(1).expect("output directory");
    fs::create_dir_all(&root).unwrap();
    let project=ProjectLocation::new(BuildFolder::new(&"a".repeat(116)).unwrap(),"p").unwrap();
    let destination=project.file("z").unwrap();
    assert_eq!(destination.as_str().len(),120);
    let mut work=Work::new(1,"Show the whole project destination",&["Preview file"],true).unwrap();
    assert!(work.observe(1,1,Update::Begin));
    assert!(work.observe(1,2,Update::Start{step:0}));
    assert!(work.observe(1,3,Update::Preview{revision:u64::MAX,location:destination.as_str(),
        change:ChangeState::Proposed,pixels:&[GROUND;PREVIEW_PIXELS]}));
    for (width,height) in [(640,480),(800,600),(1024,768),(1920,1080)] {
        let mut pixels=vec![GROUND;width*height];
        live_view::paint(&mut Frame::new(&mut pixels,width,height,width,GROUND,false),&work);
        let mut file=fs::File::create(Path::new(&root).join(format!("{width}x{height}.ppm"))).unwrap();
        write!(file,"P6\n{width} {height}\n255\n").unwrap();
        let rgb:Vec<u8>=pixels.iter().flat_map(|p|[(p>>16)as u8,(p>>8)as u8,*p as u8]).collect();
        file.write_all(&rgb).unwrap();
    }
}
