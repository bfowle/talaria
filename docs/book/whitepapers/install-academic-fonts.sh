#!/bin/bash

# Install professional academic fonts for Ubuntu/LaTeX/Pandoc
# This script installs high-quality fonts suitable for scientific publishing

echo "==================================================================================="
echo "Installing Professional Academic Fonts for HERALD Whitepaper"
echo "==================================================================================="
echo ""
echo "This will install:"
echo "  - STIX Two (professional scientific publishing standard)"
echo "  - TeX Gyre family (IEEE/ACM compatible alternatives)"
echo "  - Libertinus (ACM publication standard)"
echo "  - Latin Modern (enhanced Computer Modern)"
echo "  - Liberation (metric-compatible with Times/Arial)"
echo ""
echo "Press Enter to continue or Ctrl-C to cancel..."
read

# Update package list
echo "Updating package list..."
sudo apt update

# Core TeX Live packages
echo ""
echo "Installing core TeX packages..."
sudo apt install -y texlive-latex-base texlive-latex-recommended texlive-latex-extra

# TeX engines for modern font support
echo ""
echo "Installing XeLaTeX/LuaLaTeX support..."
sudo apt install -y texlive-xetex texlive-luatex

# Font packages
echo ""
echo "Installing font packages..."
sudo apt install -y texlive-fonts-recommended texlive-fonts-extra

# Specific high-quality fonts
echo ""
echo "Installing specific academic fonts..."

# Latin Modern - Enhanced Computer Modern
echo "  - Installing Latin Modern (traditional academic TeX)..."
sudo apt install -y lmodern

# TeX Gyre family - Professional alternatives to standard fonts
echo "  - Installing TeX Gyre family (Times, Palatino, Helvetica alternatives)..."
sudo apt install -y fonts-texgyre

# STIX fonts - Professional scientific publishing
echo "  - Installing STIX fonts (scientific symbols and math)..."
sudo apt install -y fonts-stix

# Note: STIX Two is not in standard Ubuntu repos, but STIX v1 is available
# For STIX Two, manual installation would be needed:
if ! fc-list | grep -q "STIX Two"; then
    echo ""
    echo "Note: STIX Two fonts are not in standard Ubuntu repositories."
    echo "STIX v1 has been installed, which provides good Unicode support."
    echo "For STIX Two, you would need to download from: https://github.com/stipub/stixfonts"
fi

# Liberation fonts - Metric-compatible with Microsoft fonts
echo "  - Installing Liberation fonts (Times/Arial replacements)..."
sudo apt install -y fonts-liberation fonts-liberation2

# Linux Libertine/Biolinum
echo "  - Installing Linux Libertine (elegant serif)..."
sudo apt install -y fonts-linuxlibertine

# Libertinus - Enhanced fork of Linux Libertine
echo "  - Checking for Libertinus fonts..."
if apt-cache show fonts-libertinus >/dev/null 2>&1; then
    echo "  - Installing Libertinus (ACM standard)..."
    sudo apt install -y fonts-libertinus
else
    echo "  - Libertinus not available in this Ubuntu version"
    echo "    (requires Ubuntu 20.04+ or manual installation)"
fi

# Font configuration tools
echo ""
echo "Installing font configuration tools..."
sudo apt install -y fontconfig

# Refresh font cache
echo ""
echo "Refreshing font cache..."
sudo fc-cache -fv

# Verification
echo ""
echo "==================================================================================="
echo "Installation Complete!"
echo "==================================================================================="
echo ""
echo "Installed fonts summary:"
echo ""

# Check what's actually installed
echo "Professional fonts available:"
fc-list | grep -i "latin modern roman" | head -1 && echo "  ✓ Latin Modern (traditional academic)"
fc-list | grep -i "tex gyre termes" | head -1 && echo "  ✓ TeX Gyre Termes (Times alternative)"
fc-list | grep -i "tex gyre pagella" | head -1 && echo "  ✓ TeX Gyre Pagella (Palatino alternative)"
fc-list | grep -i "tex gyre heros" | head -1 && echo "  ✓ TeX Gyre Heros (Helvetica alternative)"
fc-list | grep -i "stix" | head -1 && echo "  ✓ STIX (scientific symbols)"
fc-list | grep -i "libertinus serif" | head -1 && echo "  ✓ Libertinus (ACM standard)"
fc-list | grep -i "linux libertine" | head -1 && echo "  ✓ Linux Libertine (elegant serif)"
fc-list | grep -i "liberation serif" | head -1 && echo "  ✓ Liberation (MS font metrics)"

echo ""
echo "The build script will automatically detect and use the best available fonts."
echo "Priority order:"
echo "  1. STIX Two (if manually installed)"
echo "  2. TeX Gyre Termes (IEEE standard)"
echo "  3. Libertinus (ACM standard)"
echo "  4. TeX Gyre Pagella (elegant)"
echo "  5. Latin Modern (traditional)"
echo ""
echo "To test the PDF generation, run: ./build-paper.sh"
echo ""