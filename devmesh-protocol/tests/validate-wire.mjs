import {readFileSync} from 'node:fs';
import Ajv from 'ajv/dist/2020.js';
import formats from 'ajv-formats';
const ajv=new Ajv({strict:false,allErrors:true});formats(ajv);
const input=JSON.parse(readFileSync(0,'utf8'));const schema=JSON.parse(readFileSync(new URL('../schemas/wire.schema.json',import.meta.url),'utf8'));
schema.oneOf=schema.oneOf.filter(branch=>branch.properties.type.const===input.type);
const validate=ajv.compile(schema);const valid=validate(input);
if(!valid){console.error(JSON.stringify(validate.errors));process.exitCode=1;}
