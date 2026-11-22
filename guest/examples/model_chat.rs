//! Host-only full text-path check. The 8B model requires explicit project-trial mode.
//! No dev/model, dev/talk, UI, native boot or release-performance claim.
use std::{fs::File,io::{Read,Write},time::Instant};
use pcpos_model_media::{Manifest,VerifiedSlices};
use pcpos_qwen::{Model,forward::Decoder,tokenizer::{Tokenizer,Utf8Stream,MAX_TOKEN_BYTES}};
use tailorbird_ui::model_session::{Memory,greedy,template_supported,CONTEXT};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let a:Vec<_>=std::env::args().skip(1).collect();
    let project_trial = a.len() == 5 && a[4] == "--project-trial";
    if a.len() != 4 && !project_trial { return Err("model_chat MODEL MANIFEST NEW_OUT MESSAGE [--project-trial]".into()); }
    let mut f=File::open(&a[0])?;
    let n=f.metadata()?.len();
    let pin_hex = match (n, project_trial) {
        (491_400_064, _) => "bbdaa8cd263d9454159f12b55ec32613d18a4b7b4d75d284ad0cadc932bc1f4b",
        (5_027_783_488, true) => "2c02ff6f00a2ee802ce09648dfbc56dda38a64b009635f0aeb383f09e1af67dc",
        _ => return Err("unsupported model size for the selected trial".into()),
    };
    let mut pin = [0; 32];
    for (i, byte) in pin.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&pin_hex[2*i..2*i+2], 16)?;
    }
    let manifest=std::fs::read(&a[1])?;
    let m=Manifest::parse(&manifest,pin).map_err(|e|format!("manifest {e:?}"))?;
    if m.model_bytes() != n { return Err("manifest length differs from the model".into()); }
    let len = usize::try_from(n)?;
    let mut b = Vec::new();
    b.try_reserve_exact(len)?;
    b.resize(len, 0);
    f.read_exact(&mut b)?;
    if f.read(&mut [0])? != 0 { return Err("model length changed".into()); }
    let shards:Vec<_>=b.chunks(m.shard_bytes() as usize).collect();let _v=VerifiedSlices::verify(&m,&shards,|_|true).map_err(|e|format!("weights {e:?}"))?;
    let i=pcpos_gguf::Index::parse(&b,n).map_err(|e|format!("index {e:?}"))?;
    let model=Model::admit(&i).map_err(|e|format!("model {e:?}"))?;if !template_supported(&i,model.config().architecture){return Err("template".into());}
    let tok=Tokenizer::new(&i,16<<20,||true).map_err(|e|format!("tokenizer {e:?}"))?;
    let memory=Memory::new().map_err(|e|format!("memory {e:?}"))?;let turn=memory.begin(1,&a[3]).map_err(|e|format!("turn {e:?}"))?;
    let prompt=memory.prompt(&tok,model.config().architecture,&turn,false,||true).map_err(|e|format!("prompt {e:?}"))?;
    let mut decoder=Decoder::new(&model,&b,CONTEXT,768<<20,||true).map_err(|e|format!("decoder {e:?}"))?;
    std::fs::create_dir(&a[2])?;let started=Instant::now();
    let mut next=greedy(decoder.prefill(&prompt,||true).map_err(|e|format!("prefill {e:?}"))?,&tok);
    let prefill=started.elapsed().as_secs_f64();let mut stream=Utf8Stream::new();let mut piece=[0;MAX_TOKEN_BYTES];let mut answer=String::new();let mut ids=Vec::new();let mut ended=false;let mut first=None;
    let limit = if project_trial { tailorbird_ui::model_session::REPLY_TOKENS } else { 16 };
    for _ in 0..limit {
        let id=next.ok_or("sampling")?;if id==tok.eos()||id==tok.bos(){ended=true;break;}
        ids.push(id);let n=tok.piece(id,&mut piece).map_err(|e|format!("piece {e:?}"))?;
        stream.push(&piece[..n],|s|{if first.is_none(){first=Some(started.elapsed().as_secs_f64());}answer.push_str(s);}).map_err(|e|format!("UTF8 {e:?}"))?;
        next=greedy(decoder.step(id,||true).map_err(|e|format!("decode {e:?}"))?,&tok);
    }
    let result=format!("scope=host-full-text-smoke-not-native\nprompt_tokens={}\ngenerated_tokens={}\neos={}\nutf8_complete={}\nprefill_seconds={prefill:.6}\nfirst_decoded_text_seconds={first:?}\ntotal_seconds={:.6}\nids={ids:?}\n",prompt.len(),ids.len(),ended,stream.finish().is_ok(),started.elapsed().as_secs_f64());
    std::fs::write(std::path::Path::new(&a[2]).join("answer.txt"),answer.as_bytes())?;std::fs::write(std::path::Path::new(&a[2]).join("result.txt"),result.as_bytes())?;
    std::io::stdout().write_all(result.as_bytes())?;std::io::stdout().write_all(answer.as_bytes())?;Ok(())
}
