//! Bounded conversation memory and explicit Qwen chat framing for the qualification worker.
//! No I/O, tool calls or authority. Single requests ignore retained history when answering
//! but remain part of the conversation; only explicit FORGET removes that memory.
use alloc::{string::String,vec::Vec};
use pcpos_qwen::{Architecture,tokenizer::Tokenizer};
use pcpos_gguf::{Index,Value};
pub const CONTEXT:usize=2048;
pub const REPLY_TOKENS:usize=128;
pub const REPLY_BYTES:usize=4096;
const MEMORY_BYTES:usize=8192;
const MESSAGES:usize=32;
const SYSTEM:&str="You are Taylor, the PCP OS assistant. Be helpful and concise. Report only actions actually completed.";

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Error{Allocation,Full,Template,Tokenizer,Cancelled}
/// This worker implements the chat-only branch of these two exact templates.
/// Arbitrary Jinja from a model is never evaluated or silently approximated.
pub fn template_supported(index:&Index<'_>,arch:Architecture)->bool{
    let Some(Value::String(template))=index.get("tokenizer.chat_template") else{return false;};
    let digest=crate::sha256::sha256(&[template.as_bytes()]);
    let want=match arch{
        Architecture::Qwen2=>[0xcd,0x8e,0x94,0x39,0xf0,0x57,0x08,0x56,0xfd,0x70,0x47,0x0b,0xf8,0x88,0x9e,0xbd,0x8b,0x5d,0x11,0x07,0x20,0x7f,0x67,0xa5,0xef,0xb4,0x6e,0x34,0x23,0x30,0x52,0x7f],
        Architecture::Qwen3=>[0x57,0xf1,0xfd,0x00,0xf0,0x01,0x3a,0x2b,0xe9,0x6a,0xa7,0x9b,0x85,0x73,0x91,0xf2,0x7e,0x23,0xdf,0x5b,0x5f,0x84,0x70,0x72,0xb5,0x24,0xc8,0x97,0xe2,0x4d,0x03,0x61],
    };digest==want
}
pub struct Memory {conversation:u64,messages:Vec<String>,bytes:usize}
pub struct Turn {pub conversation:u64,pub user:String,pub answer:String}
impl Memory {
    pub fn conversation(&self)->u64{self.conversation}
    pub fn new()->Result<Self,Error>{let mut messages=Vec::new();messages.try_reserve_exact(MESSAGES).map_err(|_|Error::Allocation)?;Ok(Self{conversation:0,messages,bytes:0})}
    pub fn forget(&mut self,id:u64){if self.conversation==id{self.messages.clear();self.bytes=0;self.conversation=0;}}
    pub fn begin(&self,id:u64,user:&str)->Result<Turn,Error>{
        let (bytes,count)=if id==self.conversation{(self.bytes,self.messages.len())}else{(0,0)};
        if id==0||count+2>MESSAGES||bytes+user.len()+REPLY_BYTES>MEMORY_BYTES{return Err(Error::Full);}
        let mut saved=String::new();saved.try_reserve_exact(user.len()).map_err(|_|Error::Allocation)?;saved.push_str(user);
        let mut answer=String::new();answer.try_reserve_exact(REPLY_BYTES).map_err(|_|Error::Allocation)?;
        Ok(Turn{conversation:id,user:saved,answer})
    }
    /// Commit the emitted response, including a partial response on Stop/failure.
    /// Caller must discard the Turn if FORGET arrived while it was being answered.
    pub fn finish(&mut self,turn:Turn)->Result<(),Error>{
        if turn.answer.len()>REPLY_BYTES{return Err(Error::Full);}
        let (bytes,count)=if turn.conversation==self.conversation{(self.bytes,self.messages.len())}else{(0,0)};
        if turn.conversation==0||count+2>MESSAGES||bytes+turn.user.len()+turn.answer.len()>MEMORY_BYTES{return Err(Error::Full);}
        if turn.conversation!=self.conversation{self.messages.clear();self.bytes=0;self.conversation=turn.conversation;}
        self.bytes+=turn.user.len()+turn.answer.len();self.messages.push(turn.user);self.messages.push(turn.answer);Ok(())
    }
    pub fn prompt(&self,tok:&Tokenizer<'_>,arch:Architecture,turn:&Turn,history:bool,mut keep:impl FnMut()->bool)->Result<Vec<u32>,Error>{
        let start=tok.special_id("<|im_start|>").ok_or(Error::Template)?;
        let end=tok.special_id("<|im_end|>").ok_or(Error::Template)?;
        if end!=tok.eos()||tok.special_id("<|endoftext|>")!=Some(tok.bos()){return Err(Error::Template);}
        let mut out=Vec::new();out.try_reserve_exact(CONTEXT).map_err(|_|Error::Allocation)?;
        let mut add=|role:&str,content:&str,closed:bool|->Result<(),Error>{
            if !keep(){return Err(Error::Cancelled);}
            let mut body=String::new();body.try_reserve_exact(role.len()+1+content.len()).map_err(|_|Error::Allocation)?;
            body.push_str(role);body.push('\n');body.push_str(content);
            let ids=tok.encode(&body,CONTEXT,4<<20,&mut keep).map_err(|e|if e==pcpos_qwen::tokenizer::Error::Cancelled{Error::Cancelled}else{Error::Tokenizer})?;
            let newline=if closed{tok.encode("\n",1,4096,&mut keep).map_err(|_|Error::Tokenizer)?}else{Vec::new()};
            if out.len()+1+ids.len()+usize::from(closed)+newline.len()>CONTEXT-REPLY_TOKENS{return Err(Error::Full);}
            out.push(start);out.extend_from_slice(&ids);if closed{out.push(end);out.extend_from_slice(&newline);}Ok(())
        };
        add("system",SYSTEM,true)?;
        if history && self.conversation==turn.conversation {
            for (i,s) in self.messages.iter().enumerate(){add(if i%2==0{"user"}else{"assistant"},s,true)?;}
        }
        add("user",&turn.user,true)?;add("assistant","",false)?;
        // Qwen3's official enable_thinking=false generation prefix, inserted explicitly.
        if arch==Architecture::Qwen3 {
            for (name,after) in [("<think>","\n\n"),("</think>","\n\n")] {
                let id=tok.special_id(name).ok_or(Error::Template)?;
                let ids=tok.encode(after,8,4096,&mut keep).map_err(|_|Error::Tokenizer)?;
                if out.len()+1+ids.len()>CONTEXT-REPLY_TOKENS{return Err(Error::Full);}
                out.push(id);out.extend_from_slice(&ids);
            }
        }
        Ok(out)
    }
}

/// Deterministic qualification sampling; lowest ID wins a tie. This is not a
/// selected release sampling policy. Unused/other control tokens are excluded.
pub fn greedy(logits:&[f32],tok:&Tokenizer<'_>)->Option<u32>{
    if logits.len()!=tok.vocabulary(){return None;}
    let mut best=None;
    for (i,&x) in logits.iter().enumerate(){
        if !x.is_finite(){return None;}
        if (tok.is_text(i as u32)||i as u32==tok.eos()||i as u32==tok.bos()) && best.is_none_or(|(_,v)|x>v){best=Some((i as u32,x));}
    }
    best.map(|p|p.0)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn partial_turns_remain_and_forget_names_only_one_conversation(){
        let mut m=Memory::new().unwrap();let mut t=m.begin(7,"hello").unwrap();t.answer.push_str("part");m.finish(t).unwrap();
        assert_eq!(m.messages[0],"hello");assert_eq!(m.messages[1],"part");
        m.forget(8);assert_eq!(m.messages.len(),2);m.forget(7);assert!(m.messages.is_empty());assert_eq!(m.bytes,0);
    }
    #[test] fn future_single_turns_do_not_discard_prior_memory_and_limits_refuse(){
        let mut m=Memory::new().unwrap();
        for _ in 0..16 {let t=m.begin(1,"x").unwrap();m.finish(t).unwrap();}
        assert!(matches!(m.begin(1,"x"),Err(Error::Full)));assert!(m.begin(2,"new").is_ok());
        m.forget(1);let mut t=m.begin(1,"first").unwrap();t.answer.push_str(&"a".repeat(REPLY_BYTES));m.finish(t).unwrap();
        assert!(matches!(m.begin(1,"next"),Err(Error::Full)));assert_eq!(m.messages.len(),2);
    }
}
