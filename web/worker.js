let api;
const encoder=new TextEncoder(),decoder=new TextDecoder();
function result(n){return new Uint8Array(api.memory.buffer,api.output_ptr(),n).slice();}
function callInput(fn,bytes){const p=api.alloc(bytes.length);new Uint8Array(api.memory.buffer,p,bytes.length).set(bytes);try{return result(api[fn](p,bytes.length));}finally{api.dealloc(p,bytes.length);}}
self.onmessage=async({data:{id,op,value}})=>{try{
 if(!api){const response=await fetch('core.wasm');if(!response.ok)throw new Error(`Core fetch failed: ${response.status}`);const {instance}=await WebAssembly.instantiate(await response.arrayBuffer(),{});api=instance.exports;}
 let bytes;
 switch(op){case'init':bytes=result(api.init(value?1:0));break;case'command':bytes=callInput('command',encoder.encode(value));break;case'status':bytes=result(api.status());break;case'save':bytes=result(api.save());break;case'load':bytes=callInput('load',new Uint8Array(value));break;case'file':bytes=callInput('get_file',encoder.encode(value));break;default:throw new Error('Unknown worker operation');}
 if(op==='save'||op==='file'){self.postMessage({id,bytes},[bytes.buffer]);}else{self.postMessage({id,text:decoder.decode(bytes)});}
 }catch(error){self.postMessage({id,error:String(error)});}};
