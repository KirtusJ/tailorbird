//! Shared, host-tested request state for the native model worker. No device I/O.
use crate::wire::{Kind,Reader,Step,MOST};
pub struct Request{pub conversation:u64,pub reply:u64,pub history:bool,pub progress:bool,pub body:[u8;MOST],pub length:usize}
pub struct Inbox{reader:Reader,pub pending:Option<Request>,pub stopped:bool,pub remembered:u64,pub forgotten:Option<u64>,pub forget_active:bool}
impl Default for Inbox{fn default()->Self{Self::new()}}
impl Inbox{
    pub fn new()->Self{Self{reader:Reader::new(),pending:None,stopped:false,remembered:0,forgotten:None,forget_active:false}}
    pub fn take(&mut self,byte:u8,active:Option<(u64,u64)>)->Result<(),&'static str>{
        match self.reader.take(byte){
            Step::More=>Ok(()),Step::Broken=>Err("malformed conversation record"),Step::Whole=>{
                let r=self.reader.record().ok_or("missing conversation record")?;
                match r.kind{
                    Kind::Say=>{
                        if self.pending.is_some(){return Err("multiple queued requests");}
                        let mut body=[0;MOST];body[..r.body.len()].copy_from_slice(r.body);
                        self.pending=Some(Request{conversation:r.conversation,reply:r.reply,history:r.history,progress:self.reader.progress_wanted(),body,length:r.body.len()});
                    },
                    Kind::Stop=>{
                        if active==Some((r.conversation,r.reply)){self.stopped=true;}
                        if self.pending.as_ref().is_some_and(|p|p.conversation==r.conversation&&p.reply==r.reply){self.pending=None;}
                    },
                    Kind::Forget=>{
                        // Only one conversation is retained. Latch its removal;
                        // a later unrelated FORGET must not overwrite that request.
                        if r.conversation==self.remembered{self.forgotten=Some(r.conversation);}
                        if active.is_some_and(|p|p.0==r.conversation){self.forget_active=true;self.stopped=true;}
                        if self.pending.as_ref().is_some_and(|p|p.conversation==r.conversation){self.pending=None;}
                    },
                    _=>return Err("record direction refused"),
                }Ok(())
            }
        }
    }
}
pub struct Chunks<'a>{rest:&'a str}
pub fn chunks(text:&str)->Chunks<'_>{Chunks{rest:text}}
impl<'a> Iterator for Chunks<'a>{type Item=&'a str;fn next(&mut self)->Option<Self::Item>{
    if self.rest.is_empty(){return None;}
    let mut n=self.rest.len().min(MOST);while !self.rest.is_char_boundary(n){n-=1;}
    let (front,rest)=self.rest.split_at(n);self.rest=rest;Some(front)
}}
#[cfg(test)] mod tests{
    use super::*;use crate::wire::{pack,Record};
    fn feed(i:&mut Inbox,k:Kind,c:u64,r:u64,active:Option<(u64,u64)>)->Result<(),&'static str>{
        let p=pack(Record{kind:k,history:false,conversation:c,reply:r,body:b""}).unwrap();
        for b in p.bytes(){i.take(*b,active)?;}Ok(())
    }
    #[test] fn queued_stop_and_forget_cancel_before_any_inference(){
        for kind in [Kind::Stop,Kind::Forget]{let mut i=Inbox::new();feed(&mut i,Kind::Say,7,2,None).unwrap();
            feed(&mut i,kind,7,if kind==Kind::Forget{0}else{2},None).unwrap();assert!(i.pending.is_none());}
    }
    #[test] fn cancellation_names_the_active_reply_and_remains_latched(){
        let mut i=Inbox::new();feed(&mut i,Kind::Stop,7,1,Some((7,2))).unwrap();assert!(!i.stopped);
        feed(&mut i,Kind::Forget,7,0,Some((7,2))).unwrap();feed(&mut i,Kind::Forget,8,0,Some((7,2))).unwrap();
        assert!(i.stopped&&i.forget_active);
        let mut i=Inbox::new();feed(&mut i,Kind::Say,9,3,Some((7,2))).unwrap();feed(&mut i,Kind::Stop,7,2,Some((7,2))).unwrap();
        assert!(i.stopped);assert_eq!(i.pending.unwrap().conversation,9);
    }
    #[test] fn wrong_direction_and_duplicate_queued_request_refuse(){
        assert!(feed(&mut Inbox::new(),Kind::Ready,0,0,None).is_err());let mut i=Inbox::new();
        feed(&mut i,Kind::Say,1,1,None).unwrap();assert!(feed(&mut i,Kind::Say,1,2,None).is_err());
    }
    #[test] fn unrelated_forget_cannot_replace_retained_memory_reset(){
        let mut i=Inbox::new();i.remembered=7;
        feed(&mut i,Kind::Forget,7,0,None).unwrap();feed(&mut i,Kind::Forget,8,0,None).unwrap();
        assert_eq!(i.forgotten,Some(7));assert!(!i.stopped);
    }
    #[test] fn multibyte_at_480_is_moved_whole_and_wire_reassembles_exactly(){
        for prefix in [477,478,479,480]{for character in ["é","中","🛠"]{
            let text=alloc::format!("{}{}{}","a".repeat(prefix),character,"b".repeat(600));let mut reader=Reader::new();let mut out=alloc::string::String::new();
            for part in chunks(&text){assert!(part.len()<=MOST);let p=pack(Record{kind:Kind::Chunk,history:false,conversation:1,reply:1,body:part.as_bytes()}).unwrap();
                for byte in p.bytes(){if reader.take(*byte)==Step::Whole{out.push_str(core::str::from_utf8(reader.record().unwrap().body).unwrap());}}
            }assert_eq!(out.as_bytes(),text.as_bytes());
            if prefix+character.len()>MOST && prefix<MOST {assert_eq!(chunks(&text).next().unwrap().len(),prefix);}
        }}
    }
}
