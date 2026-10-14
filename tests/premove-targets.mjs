import fs from 'node:fs';import ts from 'typescript';import assert from 'node:assert/strict';
const source=fs.readFileSync('src/premoveTargets.ts','utf8'),js=ts.transpile(source,{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022});const {premoveTargets}=await import('data:text/javascript;base64,'+Buffer.from(js).toString('base64'));
const p=(square,role,color='w')=>({square,role,color});const fen='8/8/8/8/8/8/8/8 b KQkq - 0 1';
assert.deepEqual(premoveTargets([p('g1','n'),p('e2','p')],'g1',fen).sort(),['f3','h3']);
assert.deepEqual(premoveTargets([p('e2','p')],'e2',fen).sort(),['d3','e3','e4','f3']);
assert.ok(!premoveTargets([p('e2','p'),p('e3','n')],'e2',fen).includes('e4'));
assert.deepEqual(premoveTargets([p('e7','p','b')],'e7',fen).sort(),['d6','e5','e6','f6']);
assert.ok(premoveTargets([p('e5','p')],'e5',fen).includes('d6'));
assert.ok(premoveTargets([p('a7','p')],'a7',fen).includes('a8'));
assert.ok(!premoveTargets([p('a1','r'),p('a3','p')],'a1',fen).includes('a4'));
assert.ok(premoveTargets([p('a1','r'),p('a3','p','b')],'a1',fen).includes('a4'));
assert.ok(premoveTargets([p('e1','k'),p('h1','r'),p('a1','r')],'e1',fen).includes('g1'));
assert.ok(!premoveTargets([p('e1','k'),p('h1','r')],'e1',fen.replace('KQkq','-')).includes('g1'));
assert.ok(premoveTargets([p('b1','k'),p('h1','r')],'b1',fen.replace('KQkq','H'),true).includes('h1'));
console.log('11 premove destination checks passed');

