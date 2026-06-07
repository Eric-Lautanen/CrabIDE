const fs = require('fs');

// Count all_actions entries that use Action::
function findMissing() {
    const content = fs.readFileSync('crates/crabide-config/src/keybindings.rs', 'utf-8');
    const lines = content.split('\n');
    
    // Extract enum variants
    let inEnum = false;
    let variants = [];
    for (const line of lines) {
        if (line.includes('pub enum Action')) { inEnum = true; continue; }
        if (inEnum) {
            if (line.trim() === '}') break;
            if (line.trim().startsWith('//') || line.trim() === '') continue;
            const part = line.trim().split('(')[0].split(',')[0].split(' ').filter(s => s.length > 0);
            if (part.length > 0) {
                const variant = part[0];
                if (variant[0] >= 'A' && variant[0] <= 'Z') variants.push(variant);
            }
        }
    }
    
    // Extract all Action:: references in the file (excluding InsertText and Custom)
    const actionRefs = new Set();
    for (const line of lines) {
        const m = line.match(/Action::([A-Za-z]+)/g);
        if (m) {
            for (const a of m) {
                const name = a.replace('Action::', '');
                actionRefs.add(name);
            }
        }
    }
    
    const missing = variants.filter(v => !actionRefs.has(v));
    console.log('Total enum variants (excl InsertText/Custom):', variants.length);
    console.log('Total Action:: references in file:', actionRefs.size);
    console.log('Missing from all_actions():', missing);
}

findMissing();
