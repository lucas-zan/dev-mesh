import test from 'node:test';
import assert from 'node:assert/strict';
import {checkSteer} from '../src/control.ts';
test('extension refuses idle and old epochs before any native input',()=>{
 assert.throws(()=>checkSteer(true,2,2),/invalid_state/);
 assert.throws(()=>checkSteer(false,2,1),/stale_turn/);
 assert.doesNotThrow(()=>checkSteer(false,2,2));
});
