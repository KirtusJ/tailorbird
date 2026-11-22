#![no_std]
#![no_main]
//! Opt-in scalar inference qualification worker. No tool execution or release-speed claim.
extern crate alloc;
#[path="../../pcp-os/store/apps/parrots-all-the-way-down/guest/src/heap.rs"] mod heap;
mod model_worker_io;
#[cfg(all(target_os = "linux", feature = "stable-model-worker"))]
mod stable_compare;
use model_worker_io::{Link,Mapping,Result,sleep};
use pcp_game_guest::{exit,log};
use pcpos_model_media::{Manifest,VerifiedSlices};
use pcpos_qwen::{Model,forward::Decoder,tokenizer::{Tokenizer,Utf8Stream,MAX_TOKEN_BYTES}};
use tailorbird_ui::{model_session::{Memory,greedy,template_supported,CONTEXT,REPLY_TOKENS,REPLY_BYTES},wire::Kind};

pcp_game_guest::entry!(worker_main);
#[no_mangle] pub extern "C" fn worker_main()->!{
    let mut link=match Link::open(){Ok(l)=>l,Err(e)=>fail(e)};
    if let Err(e)=run(&mut link){let _=link.send(Kind::NotStarted,None,b"");fail(e);}
    exit(0)
}
fn fail(reason:&str)->!{log(b"Taylor model worker: ");log(reason.as_bytes());log(b"\n");exit(2)}
fn run(link:&mut Link)->Result<()>{
    link.send(Kind::Loading,None,b"")?;
    let mapping=Mapping::open()?;let bytes=mapping.bytes();
    let manifest=Manifest::parse(&mapping.manifest,mapping.pin).map_err(|_|"manifest refused")?;
    let mut shards=alloc::vec::Vec::new();shards.try_reserve_exact(manifest.shard_count() as usize).map_err(|_|"shard table memory")?;
    shards.extend(bytes.chunks(manifest.shard_bytes() as usize));
    let mut last=0;let mut transport_ok=true;
    let _verified=VerifiedSlices::verify(&manifest,&shards,|p|{
        if p.bytes_read==0||p.bytes_read-last>=64<<20||p.verified_shards==manifest.shard_count(){
            let mut body=[0;16];body[..8].copy_from_slice(&p.bytes_read.to_le_bytes());body[8..].copy_from_slice(&p.total_bytes.to_le_bytes());
            transport_ok=link.send(Kind::Loading,None,&body).is_ok();last=p.bytes_read;
        }transport_ok
    }).map_err(|_|"model verification failed or channel ended")?;
    let index=pcpos_gguf::Index::parse(bytes,bytes.len() as u64).map_err(|_|"GGUF index refused")?;
    let model=Model::admit(&index).map_err(|_|"Qwen architecture refused")?;
    if !template_supported(&index,model.config().architecture){return Err("unqualified chat template");}
    let mut ticks=0;
    let tokenizer=Tokenizer::new(&index,16<<20,||link.keep(None,&mut ticks)).map_err(|_|"Qwen tokenizer refused")?;
    if tokenizer.vocabulary()!=model.config().vocabulary||model.config().training_context<CONTEXT{return Err("model context/vocabulary unsuitable");}
    let mut memory=Memory::new().map_err(|_|"conversation memory")?;
    let probe=memory.begin(1,"Hi").map_err(|_|"conversation preflight")?;
    let prompt=memory.prompt(&tokenizer,model.config().architecture,&probe,false,||true).map_err(|_|"chat profile refused")?;
    let mut decoder=Decoder::new(&model,bytes,CONTEXT,768<<20,||link.keep(None,&mut ticks)).map_err(|_|"model working memory refused")?;
    decoder.step(prompt[0],||link.keep(None,&mut ticks)).map_err(|_|"model preflight failed")?;decoder.reset();
    drop(prompt);drop(probe);
    link.send(Kind::Ready,None,b"")?;log(b"Taylor model worker: verified, tokenized and preflighted; scalar qualification only\n");
    loop{
        link.state.remembered=memory.conversation();
        link.poll(None)?;
        if let Some(id)=link.state.forgotten.take(){memory.forget(id);}
        let Some(got)=link.state.pending.take() else {sleep()?;continue};
        link.state.stopped=false;link.state.forget_active=false;
        let active=Some((got.conversation,got.reply));
        let user=match core::str::from_utf8(&got.body[..got.length]){Ok(s)=>s,Err(_)=>{link.send(Kind::Failed,active,b"")?;continue;}};
        let mut turn=match memory.begin(got.conversation,user){Ok(t)=>t,Err(_)=>{notice(link,active,b"This conversation is full. Start a new chat to continue.")?;continue;}};
        let prompt=match memory.prompt(&tokenizer,model.config().architecture,&turn,got.history,||link.keep(active,&mut ticks)){
            Ok(p)=>p,Err(_)=>{if link.state.forget_active{memory.forget(got.conversation);}if !link.state.stopped{notice(link,active,b"This message cannot fit the current conversation. Start a new chat or use Single mode.")?;}continue;}
        };
        decoder.reset();let mut ended=false;
        let mut token=match decoder.prefill(&prompt,||link.keep(active,&mut ticks)){
            Ok(logits)=>greedy(logits,&tokenizer),Err(_)=>None,
        };
        let mut failed=token.is_none();
        drop(prompt);
        let mut utf8=Utf8Stream::new();let mut piece=[0;MAX_TOKEN_BYTES];
        for _ in 0..REPLY_TOKENS {
            link.poll(active)?;if failed||link.state.stopped{break;}
            let Some(id)=token else {failed=true;break;};
            if id==tokenizer.eos()||id==tokenizer.bos(){ended=utf8.finish().is_ok();failed=!ended;break;}
            let n=match tokenizer.piece(id,&mut piece){Ok(n)=>n,Err(_)=>{failed=true;break;}};
            let mut output_ok=true;
            if utf8.push(&piece[..n],|text|{
                if turn.answer.len()+text.len()>REPLY_BYTES{output_ok=false;return;}
                for part in tailorbird_ui::model_inbox::chunks(text){
                    match link.send(Kind::Chunk,active,part.as_bytes()){
                        Ok(true)=>turn.answer.push_str(part),Ok(false)=>{output_ok=false;break;},Err(_)=>{link.failed=true;output_ok=false;break;}
                    }
                }
            }).is_err()||!output_ok{failed=true;break;}
            match decoder.step(id,||link.keep(active,&mut ticks)){Ok(logits)=>token=greedy(logits,&tokenizer),Err(_)=>{failed=true;break;}}
        }
        if link.failed{return Err("conversation channel failed during inference");}
        link.poll(active)?;
        if link.state.forget_active {memory.forget(got.conversation);} else {memory.finish(turn).map_err(|_|"conversation commit refused")?;}
        if let Some(id)=link.state.forgotten.take(){memory.forget(id);}
        if !link.state.stopped {
            if ended&&!failed {link.send(Kind::Done,active,b"")?;}
            else {notice(link,active,if failed{b"\nThe response could not be completed."}else{b"\nResponse limit reached."})?;}
        }
        decoder.reset();
    }
}
fn notice(link:&mut Link,active:Option<(u64,u64)>,message:&[u8])->Result<()>{
    if link.send(Kind::Chunk,active,message)?{link.send(Kind::Failed,active,b"")?;}Ok(())
}
