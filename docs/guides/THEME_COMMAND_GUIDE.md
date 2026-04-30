# Theme Command Reference Guide

## Quick Start

### List All Themes
```
/theme
```

### Switch to a Theme
```
/theme tokyo-night
/theme dracula
/theme github-dark
```

### Cycle Through Themes
```
/theme next    # Go to next theme
/theme prev    # Go to previous theme
```

### Quick Theme Selection
```
/theme light    # Switch to a light theme
/theme dark     # Switch to a dark theme
/theme random   # Random theme
```

## Available Themes (22 Total)

### Dark Themes
1. **tokyo-night** - Blue-purple dark theme
2. **dracula** - Classic purple dark theme
3. **monokai** - Vibrant dark theme
4. **nord** - Arctic bluish-gray
5. **gruvbox-dark** - Retro warm dark
6. **catppuccin-mocha** - Dark pastel
7. **rose-pine** - Natural dark theme
8. **solarized-dark** - Precision dark
9. **github-dark** - GitHub's dark mode
10. **vscode-dark** - VSCode default dark
11. **atom-one-dark** - Atom's dark theme
12. **material-dark** - Material Design dark
13. **palenight** - Smooth dark theme
14. **ayu-dark** - Modern dark theme
15. **night-owl** - Accessible dark theme
16. **vesper** - Calm dark theme
17. **carbonfox** - High-contrast dark

### Light Themes
1. **oc-1** - Kilocode's light theme
2. **gruvbox-light** - Retro warm light
3. **catppuccin-latte** - Light pastel
4. **rose-pine-dawn** - Natural light theme
5. **solarized-light** - Precision light
6. **github-light** - GitHub's light mode
7. **vscode-light** - VSCode default light
8. **atom-one-light** - Atom's light theme

## Theme Preview Format

```
=== Available Themes ===
   1. tokyo-night         [■ #7aa2f7 ■ #9ece6a ■ #f7768e] Dark
      └─ Primary color   └─ Success   └─ Error    └─ Style
```

The color swatches show:
- **First color**: Primary accent color
- **Second color**: Success (green)
- **Third color**: Error (red)

## Theme Persistence

Your theme choice is automatically saved and restored on startup.

## Tips

1. **Find your favorite**: Use `/theme next` to cycle through all themes
2. **Quick access**: Use `/theme dark` or `/theme light` to quickly switch modes
3. **Random discovery**: Use `/theme random` to discover new themes
4. **See all themes**: Use `/theme` to list all 22 themes with previews

## Keyboard Shortcuts (Future)

Planned keyboard shortcuts for theme switching:
- `Alt+T` - Cycle to next theme
- `Alt+Shift+T` - Cycle to previous theme
- `Alt+L` - Switch to light theme
- `Alt+D` - Switch to dark theme

## Custom Themes (Future)

Future support for:
- User-defined theme files
- Theme customization commands
- Theme import/export
- Theme sharing

## Accessibility

All themes meet WCAG AA standards:
- 4.5:1 contrast ratio for normal text
- 3:1 contrast ratio for large text
- 3:1 contrast ratio for interactive elements

## Troubleshooting

**Theme not applying?**
- Check the theme name spelling
- Use `/theme` to list available themes
- Try `/theme random` to test theme switching

**Colors look wrong?**
- Ensure your terminal supports 256 colors
- Check your terminal's color settings
- Try a different theme to compare

**Changes not saving?**
- Check file permissions for config directory
- Ensure config directory is writable
- Check for error messages in the TUI

## Examples

### Example 1: Try All Dark Themes
```
/theme dark           # Start with first dark theme
/theme next           # Next theme
/theme next           # Next theme
# ... repeat until you find your favorite
```

### Example 2: Compare Light Themes
```
/theme oc-1           # Try OC-1
/theme github-light   # Compare with GitHub Light
/theme solarized-light# Compare with Solarized
```

### Example 3: Random Exploration
```
/theme random         # Discover new themes
/theme random         # Try another
/theme random         # Keep exploring
```

## Theme Categories

### Popular Themes (Most Used)
- tokyo-night
- dracula
- github-dark
- vscode-dark

### Kilocode Themes
- oc-1 (light)
- tokyo-night (dark)

### High Contrast Themes
- carbonfox
- solarized-dark
- monokai

### Pastel Themes
- catppuccin-mocha
- catppuccin-latte
- rose-pine

### Retro Themes
- gruvbox-dark
- gruvbox-light
- monokai

### Natural Themes
- nord
- rose-pine
- rose-pine-dawn

### Modern Themes
- ayu-dark
- night-owl
- vesper

## Need Help?

Use `/theme` without arguments to see all available themes with color previews.
