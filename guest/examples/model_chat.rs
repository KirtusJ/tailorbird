//! Bounded host-only full text-path smoke check for the pinned 0.5B reference.
//! No dev/model, dev/talk, UI, native boot or release-performance claim.
use std::{fs::File,io::{Read,Write},time::Instant};
use pcpos_model_media::{Manifest,VerifiedSlices};
use pcpos_qwen::{Model,forward::Decoder,tokenizer::{Tokenizer,Utf8Stream,MAX_TOKEN_BYTES}};
use tailorbird_ui::model_session::{Memory,greedy,template_supported,CONTEXT};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let a:Vec<_>=std::env::args().skip(1).collect();if a.len()!=4{return Err("model_chat MODEL MANIFEST NEW_OUT MESSAGE".into());}
    let f=File::open(&a[0])?;let n=f.metadata()?.len();if n!=491400064{return Err("only the nominated 0.5B host reference is admitted by this helper".into());}
    let mut b=Vec::new();b.try_reserve_exact(n as usize)?;f.take(n+1).read_to_end(&mut b)?;if b.len() as u64!=n{return Err("model length changed".into());}
    let manifest=std::fs::read(&a[1])?;
    let pin=[0xbb,0xda,0xa8,0xcd,0x26,0x3d,0x94,0x54,0x15,0x9f,0x12,0xb5,0x5e,0xc3,0x26,0x13,0xd1,0x8a,0x4b,0x7b,0x4d,0x75,0xd2,0x84,0xad,0x0c,0xad,0xc9,0x32,0xbc,0x1f,0x4b];
    let m=Manifest::parse(&manifest,pin).map_err(|e|format!("manifest {e:?}"))?;
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
    for _ in 0..16{
        let id=next.ok_or("sampling")?;if id==tok.eos()||id==tok.bos(){ended=true;break;}
        ids.push(id);let n=tok.piece(id,&mut piece).map_err(|e|format!("piece {e:?}"))?;
        stream.push(&piece[..n],|s|{if first.is_none(){first=Some(started.elapsed().as_secs_f64());}answer.push_str(s);}).map_err(|e|format!("UTF8 {e:?}"))?;
        next=greedy(decoder.step(id,||true).map_err(|e|format!("decode {e:?}"))?,&tok);
    }
    let result=format!("scope=host-full-text-smoke-not-native\nprompt_tokens={}\ngenerated_tokens={}\neos={}\nutf8_complete={}\nprefill_seconds={prefill:.6}\nfirst_decoded_text_seconds={first:?}\ntotal_seconds={:.6}\nids={ids:?}\n",prompt.len(),ids.len(),ended,stream.finish().is_ok(),started.elapsed().as_secs_f64());
    std::fs::write(std::path::Path::new(&a[2]).join("answer.txt"),answer.as_bytes())?;std::fs::write(std::path::Path::new(&a[2]).join("result.txt"),result.as_bytes())?;
    std::io::stdout().write_all(result.as_bytes())?;std::io::stdout().write_all(answer.as_bytes())?;Ok(())
}
