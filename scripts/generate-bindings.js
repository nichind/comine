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
  // Note: TS_RS_LARGE_INT is a ts-rs v12+ feature; we use v10, so we
  // post-process the generated files to replace bigint → number instead.
  const env = {
    ...process.env,
    TS_RS_EXPORT_DIR: bindingsDir,
  };
  
  execSync('cargo test --features ts-export -- export_bindings', {
    cwd: srcTauri,
    env,
    stdio: 'inherit'
  });
  
  // ts-rs v10 maps u64/i64/u128/i128 to bigint, but JavaScript uses number
  // (f64) which is sufficient for our use cases (file sizes, timestamps, etc.).
  console.log('\nPost-processing: replacing bigint → number in generated bindings...');
  const fs = await import('fs');
  const globPattern = path.join(bindingsDir, '*.ts');
  const files = fs.readdirSync(bindingsDir).filter(f => f.endsWith('.ts'));
  let replacedCount = 0;
  for (const file of files) {
    const filePath = path.join(bindingsDir, file);
    const content = fs.readFileSync(filePath, 'utf-8');
    // Replace standalone 'bigint' type references (not inside words)
    const updated = content.replace(/\bbigint\b/g, 'number');
    if (content !== updated) {
      fs.writeFileSync(filePath, updated, 'utf-8');
      replacedCount++;
      console.log(`  ✓ ${file}`);
    }
  }
  console.log(`  Processed ${replacedCount} file(s)`);
  
  console.log('\nGenerating index.ts barrel file...');
  const bindingFiles = files
    .filter(f => f !== 'index.ts')
    .sort();
  const exports = bindingFiles.map(f => {
    const name = f.replace('.ts', '');
    return `export type { ${name} } from './${name}';`;
  });
  const indexContent = [
    '// Auto-generated barrel file — do not edit manually.',
    '// Re-run: node scripts/generate-bindings.js',
    '',
    ...exports,
    '',
  ].join('\n');
  fs.writeFileSync(path.join(bindingsDir, 'index.ts'), indexContent, 'utf-8');
  console.log(`  ✓ index.ts (${bindingFiles.length} types)`);

  console.log('\n✓ TypeScript bindings generated successfully!');
  console.log(`  Check: ${bindingsDir}`);
} catch (error) {
  console.error('\n✗ Failed to generate bindings');
  process.exit(1);
}
