//! Unsafe native boundary for the opt-in model worker. The trusted kernel owns
//! the immutable, sealed, non-executable model pages for this process's lifetime.
use alloc::vec::Vec;
use pcp_game_guest::{call,open};
use tailorbird_ui::{wire::{pack,Kind,Record},model_inbox::Inbox};
pub type Result<T>=core::result::Result<T,&'static str>;

pub struct Mapping {address:usize,length:usize,pub manifest:Vec<u8>,pub pin:[u8;32]}
impl Mapping {
    pub fn open()->Result<Self>{
        let expected=pin(option_env!("PCP_MODEL_MANIFEST_SHA256").ok_or("build has no pinned model manifest")?)?;
        let fd=open(b"/dev/model\0",0);if fd<0{return Err("no loader model");}
        let result=(||{
            let mut d=[0;48];if call(16,fd as usize,0x6d01,d.as_mut_ptr() as usize,0,0,0)!=0{return Err("model descriptor refused");}
            let n=u64::from_le_bytes(d[..8].try_into().unwrap());let length=u64::from_le_bytes(d[8..16].try_into().unwrap());
            let actual:[u8;32]=d[16..48].try_into().unwrap();
            if actual!=expected||n<56||n>56+4096*32||length==0||length>pcpos_model_media::MAX_MODEL_BYTES{return Err("descriptor does not match image pin or bounds");}
            let length=usize::try_from(length).map_err(|_|"model address length")?;
            if length>isize::MAX as usize{return Err("model address length");}
            let mut manifest=Vec::new();manifest.try_reserve_exact(n as usize).map_err(|_|"manifest memory")?;manifest.resize(n as usize,0);
            let mut have=0;while have<manifest.len(){
                let got=call(0,fd as usize,manifest[have..].as_mut_ptr() as usize,manifest.len()-have,0,0,0);
                if got<=0||got as usize>manifest.len()-have{return Err("short manifest");}have+=got as usize;
            }
            let mut extra=[0u8;1];if call(0,fd as usize,extra.as_mut_ptr() as usize,1,0,0,0)!=0{return Err("extra manifest bytes");}
            let m=pcpos_model_media::Manifest::parse(&manifest,expected).map_err(|_|"manifest verification")?;
            if m.model_bytes()!=length as u64{return Err("model length mismatch");}
            let address=call(9,0,length,1,2,fd as usize,0); // exact length; PROT_READ, MAP_PRIVATE
            if address<=0{return Err("read-only model mapping refused");}
            let address=address as usize;
            if address%4096!=0||address.checked_add(length).is_none(){call(11,address,length,0,0,0,0);return Err("invalid mapping range");}
            Ok(Self{address,length,manifest,pin:expected})
        })();
        call(3,fd as usize,0,0,0,0,0);result
    }
    pub fn bytes(&self)->&[u8]{
        // #371's dev/model maps exact-length loader-owned pages, read-only/NX/SEALED,
        // excludes them from allocators, and refuses mprotect/write aliases. This
        // process creates no other model mapping. The borrow cannot outlive Drop.
        unsafe{core::slice::from_raw_parts(self.address as *const u8,self.length)}
    }
}
impl Drop for Mapping {fn drop(&mut self){call(11,self.address,self.length,0,0,0,0);}}
fn pin(s:&str)->Result<[u8;32]>{
    if s.len()!=64{return Err("image pin must be 64 lowercase hex digits");}
    let mut out=[0;32];for (i,p) in s.as_bytes().chunks_exact(2).enumerate(){
        let h=|b|match b{b'0'..=b'9'=>Ok(b-b'0'),b'a'..=b'f'=>Ok(b-b'a'+10),_=>Err("image pin hex")};out[i]=h(p[0])?*16+h(p[1])?;
    }Ok(out)
}
pub fn sleep()->Result<()>{let duration=[0u64,1_000_000];if call(35,duration.as_ptr() as usize,0,0,0,0,0)==0{Ok(())}else{Err("worker sleep refused")}}

pub struct Link {fd:isize,buffer:[u8;512],at:usize,have:usize,pub state:Inbox,pub failed:bool}
impl Link {
    pub fn open()->Result<Self>{let fd=open(b"/dev/talk\0",2|0x800);if fd<0{return Err("no conversation channel");}
        Ok(Self{fd,buffer:[0;512],at:0,have:0,state:Inbox::new(),failed:false})}
    /// Bounded bytes per poll; one queued SAY. Malformed/unexpected service records
    /// terminate the connection, rather than interpreting their payload as requests.
    pub fn poll(&mut self,active:Option<(u64,u64)>)->Result<()>{
        for _ in 0..1024 {
            if self.at==self.have {
                let n=call(0,self.fd as usize,self.buffer.as_mut_ptr() as usize,self.buffer.len(),0,0,0);
                if n== -11{return Ok(());}if n<=0||n as usize>self.buffer.len(){return Err("conversation channel ended");}
                self.at=0;self.have=n as usize;
            }
            let b=self.buffer[self.at];self.at+=1;
            self.state.take(b,active)?;
        }Ok(())
    }
    pub fn keep(&mut self,active:Option<(u64,u64)>,ticks:&mut usize)->bool{
        *ticks=ticks.wrapping_add(1);
        if *ticks%4096==0 && self.poll(active).is_err(){self.failed=true;}
        !self.failed&&!self.state.stopped
    }
    pub fn send(&mut self,kind:Kind,ids:Option<(u64,u64)>,body:&[u8])->Result<bool>{
        let (conversation,reply)=ids.unwrap_or((0,0));
        let p=pack(Record{kind,history:false,conversation,reply,body}).ok_or("outgoing record refused")?;
        let mut written=0;let mut waits=0;
        while written<p.bytes().len(){
            self.poll(ids)?;
            // A partially written record must finish to preserve framing. After it,
            // the caller observes Stop and sends no further record for that reply.
            if self.state.stopped&&ids.is_some()&&written==0{return Ok(false);}
            let rest=&p.bytes()[written..];let n=call(1,self.fd as usize,rest.as_ptr() as usize,rest.len(),0,0,0);
            if n== -11 {waits+=1;if waits>200{return Err("conversation output remained full");}sleep()?;continue;}
            if n<=0||n as usize>rest.len(){return Err("conversation write failed");}written+=n as usize;
        // The complete record was sent, even if Stop arrived during its tail.
        // Retain those sent bytes; the next send/poll observes the latched Stop.
        }Ok(true)
    }
}
impl Drop for Link {fn drop(&mut self){call(3,self.fd as usize,0,0,0,0,0);}}
