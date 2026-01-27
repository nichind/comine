#!/usr/bin/env node

/**
 * Generate TypeScript bindings from Rust types.
 * 
 * This script runs `cargo test` with the `ts-export` feature to generate
 * TypeScript types from Rust structs/enums using ts-rs.
 * 
 * Usage: node scripts/generate-bindings.js
 */

import { execSync } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const projectRoot = path.resolve(__dirname, '..');
const srcTauri = path.join(projectRoot, 'src-tauri');
const bindingsDir = path.join(projectRoot, 'src', 'lib', 'bindings');

console.log('Generating TypeScript bindings from Rust types...');
console.log(`Output directory: ${bindingsDir}\n`);

try {
  // Set TS_RS_EXPORT_DIR and run the binding generation tests
  // TS_RS_LARGE_INT=number makes u64/i64 map to number instead of bigint
  const env = {
    ...process.env,
    TS_RS_EXPORT_DIR: bindingsDir,
    TS_RS_LARGE_INT: 'number'
  };
  
  execSync('cargo test --features ts-export -- export_bindings', {
    cwd: srcTauri,
    env,
    stdio: 'inherit'
  });
  
  console.log('\n✓ TypeScript bindings generated successfully!');
  console.log(`  Check: ${bindingsDir}`);
} catch (error) {
  console.error('\n✗ Failed to generate bindings');
  process.exit(1);
}
