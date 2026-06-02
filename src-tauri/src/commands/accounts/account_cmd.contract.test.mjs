import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const types = await readFile(new URL('./account_cmd/types.rs', import.meta.url), 'utf8')
const crud = await readFile(new URL('./account_cmd/crud.rs', import.meta.url), 'utf8')

assert.match(types, /pub status: Option<String>/)
assert.match(crud, /if let Some\(status\) = params\.status \{\s*store\.accounts\[idx\]\.status = status;/s)

console.log('account_cmd update_account status contract looks correct')
