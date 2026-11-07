import fs from 'node:fs';
import assert from 'node:assert/strict';
const {instance}=await WebAssembly.instantiate(fs.readFileSync('dist/core.wasm'),{});
const api=instance.exports,enc=new TextEncoder(),dec=new TextDecoder();
function result(n){return new Uint8Array(api.memory.buffer,api.output_ptr(),n).slice();}
function input(fn,value){const bytes=typeof value==='string'?enc.encode(value):value;const p=api.alloc(bytes.length);new Uint8Array(api.memory.buffer,p,bytes.length).set(bytes);try{return result(api[fn](p,bytes.length));}finally{api.dealloc(p,bytes.length);}}
const command=s=>dec.decode(input('command',s));const status=()=>JSON.parse(dec.decode(result(api.status())));
assert.match(dec.decode(result(api.init(0))),/ORIEL-1/);
assert.equal(status().time,0);assert.equal(status().hardcore,false);
assert.match(command('send ping'),/Queued/);assert.equal(status().queued,1);
command('snapshot start');command('next');command('wait 2');assert.equal(status().contact,true);assert.match(command('rx 30'),/81/);
const snapshot=result(api.save());command('wait 20');const expected=command('log 100');
assert.match(dec.decode(input('load',snapshot)),/resumed/);command('wait 20');assert.equal(command('log 100'),expected);
command('restore start');assert.equal(status().time,0);assert.equal(status().queued,1);
assert.match(command('asm archive/bench-example.asm 0x8800 example.bin'),/Assembled/);command('lab load example.bin 0x8800');command('lab pc 0x8800');command('lab step 65');assert.match(command('lab mem 0x8700 4'),/37 00 00 00/);
assert.ok(input('get_file','archive/flight.bin').length>1000);
const broken=snapshot.slice();broken[100]^=1;assert.match(dec.decode(input('load',broken)),/checksum/);
api.init(1);assert.equal(status().hardcore,true);assert.match(command('new'),/disabled/);assert.match(dec.decode(input('load',snapshot)),/disabled/);
for(const name of ['index.html','style.css','app.js','worker.js','sw.js','icon.svg','manifest.webmanifest','README.txt'])assert.ok(fs.statSync(`dist/${name}`).size>0);
assert.ok(!fs.readFileSync('dist/sw.js','utf8').includes('__BUILD__'));
console.log('WebAssembly checks passed: commands, radio, bench, snapshots, persistence, hardcore, packaged assets.');
