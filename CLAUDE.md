To begin fuzzing, run: `cargo fuzz run <fuzz target name>`

The source code for a fuzz target by default lives in `fuzz/fuzz_targets/<fuzz target name>.rs`. 

Each fuzz target is a Rust program that is given random data and tests a crate (in this case, json-archive). Use `cargo fuzz list` to view the list of all existing fuzz targets:

## Documentation Style Guidelines

When writing command documentation, follow the Dutch engineer approach:

### Structure and Content
- Lead with what the command actually does (one clear sentence)
- Show real examples using actual demo files from the project
- Include both human-readable and machine-readable output modes
- Include practical use cases with actual bash/jq examples for scripting
- Be upfront about performance characteristics and limitations
- Document known bugs/issues directly instead of hiding them

### Tone and Communication
- Be direct and practical, not promotional
- Don't apologize for limitations. Just state them clearly so engineers can make informed decisions
- Use real scenarios, not toy examples
- Focus on what engineers actually need to know to use the tool effectively
- Include error cases and what they mean
- Provide concrete examples that can be copy-pasted and run

### Example sections to include
1. **Purpose**: One sentence explaining what it does
2. **Basic usage**: Simple examples first
3. **Output modes**: When to use human vs JSON output
4. **Practical use cases**: Real scenarios with working code
5. **Performance characteristics**: Memory/CPU usage expectations
6. **Error cases**: Common failures and what they mean
7. **Known issues**: Bugs or limitations stated directly
