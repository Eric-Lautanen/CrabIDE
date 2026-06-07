const fs = require('fs');
const content = fs.readFileSync('crates/crabide-config/src/keybindings.rs', 'utf-8');
const lines = content.split('\n');
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
console.log('Total variants:', variants.length);
// Check which variants are missing from all_actions
const insertLines = content.split('\n').filter(l => l.includes('m.insert('));
const inserted = new Set(insertLines.map(l => {
    const m = l.match(/Action::([A-Za-z]+)/);
    return m ? m[1] : null;
}).filter(Boolean));
const missing = variants.filter(v => !inserted.has(v));
console.log('Missing from all_actions:', missing);
