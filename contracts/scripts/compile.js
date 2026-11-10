#!/usr/bin/env node
'use strict';

// Compiles every contract in this directory with solc and reports real
// diagnostics. Exits non-zero if any source fails to compile, so CI cannot
// silently pass on broken contracts.

const fs = require('fs');
const path = require('path');
const solc = require('solc');

const ROOT = path.resolve(__dirname, '..');
const OUT = path.join(ROOT, 'build');

function collectSources(dir, acc = {}) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === 'build' || entry.name === 'scripts') continue;
      collectSources(full, acc);
    } else if (entry.name.endsWith('.sol')) {
      acc[path.relative(ROOT, full)] = { content: fs.readFileSync(full, 'utf8') };
    }
  }
  return acc;
}

function findImports(importPath) {
  const candidates = [
    path.join(ROOT, 'node_modules', importPath),
    path.join(ROOT, importPath),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return { contents: fs.readFileSync(candidate, 'utf8') };
    }
  }
  return { error: `File not found: ${importPath}` };
}

function main() {
  const sources = collectSources(ROOT);
  const names = Object.keys(sources).sort();
  console.log(`Compiling ${names.length} contract(s) with solc ${solc.version()}`);

  const input = {
    language: 'Solidity',
    sources,
    settings: {
      optimizer: { enabled: true, runs: 200 },
      evmVersion: 'cancun',
      outputSelection: {
        '*': { '*': ['abi', 'evm.bytecode.object', 'evm.deployedBytecode.object'] },
      },
    },
  };

  const output = JSON.parse(solc.compile(JSON.stringify(input), { import: findImports }));

  let errors = 0;
  let warnings = 0;
  for (const diagnostic of output.errors || []) {
    if (diagnostic.severity === 'error') {
      errors += 1;
      console.error(`ERROR: ${diagnostic.formattedMessage.trim()}`);
    } else {
      warnings += 1;
      if (process.env.VERBOSE) console.warn(`WARNING: ${diagnostic.formattedMessage.trim()}`);
    }
  }

  if (errors > 0) {
    console.error(`\nCompilation FAILED: ${errors} error(s), ${warnings} warning(s)`);
    process.exit(1);
  }

  fs.rmSync(OUT, { recursive: true, force: true });
  fs.mkdirSync(OUT, { recursive: true });

  let contracts = 0;
  // Several sources define same-named contracts, so artifacts are nested by
  // source directory and an index maps "dir:Contract" to the artifact path.
  const index = {};
  for (const [file, fileContracts] of Object.entries(output.contracts || {})) {
    for (const [name, data] of Object.entries(fileContracts)) {
      const bytecode = data.evm.bytecode.object;
      if (!bytecode) continue; // interface or abstract contract
      contracts += 1;
      const target = path.join(OUT, file.replace(/\.sol$/, ''), `${name}.json`);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, JSON.stringify({
        contractName: name,
        sourceFile: file,
        abi: data.abi,
        bytecode: `0x${bytecode}`,
        deployedBytecode: `0x${data.evm.deployedBytecode.object}`,
      }, null, 2));
      index[`${file.replace(/\.sol$/, '')}:${name}`] = path.relative(OUT, target);
    }
  }
  fs.writeFileSync(
    path.join(OUT, 'index.json'),
    JSON.stringify(index, null, 2),
  );

  console.log(`Compilation OK: ${contracts} deployable contract(s), ${warnings} warning(s)`);
  console.log(`Artifacts written to ${path.relative(process.cwd(), OUT)}/`);
}

main();
