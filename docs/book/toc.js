// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded affix "><a href="introduction.html">Introduction</a></li><li class="chapter-item expanded affix "><li class="part-title">User Guide</li><li class="chapter-item expanded "><a href="user-guide/quick-start.html"><strong aria-hidden="true">1.</strong> Quick Start</a></li><li class="chapter-item expanded "><a href="user-guide/cheatsheet.html"><strong aria-hidden="true">2.</strong> Cheat Sheet</a></li><li class="chapter-item expanded "><a href="user-guide/installation.html"><strong aria-hidden="true">3.</strong> Installation</a></li><li class="chapter-item expanded "><a href="user-guide/basic-usage.html"><strong aria-hidden="true">4.</strong> Basic Usage</a></li><li class="chapter-item expanded "><a href="user-guide/interactive-mode.html"><strong aria-hidden="true">5.</strong> Interactive Mode</a></li><li class="chapter-item expanded "><a href="user-guide/configuration.html"><strong aria-hidden="true">6.</strong> Configuration</a></li><li class="chapter-item expanded affix "><li class="part-title">Databases</li><li class="chapter-item expanded "><a href="databases/downloading.html"><strong aria-hidden="true">7.</strong> Downloading Databases</a></li><li class="chapter-item expanded affix "><li class="part-title">SEQUOIA Storage System</li><li class="chapter-item expanded "><a href="sequoia/introduction.html"><strong aria-hidden="true">8.</strong> What is SEQUOIA?</a></li><li class="chapter-item expanded "><a href="sequoia/concepts.html"><strong aria-hidden="true">9.</strong> Core Concepts</a></li><li class="chapter-item expanded "><a href="sequoia/how-it-works.html"><strong aria-hidden="true">10.</strong> How SEQUOIA Works</a></li><li class="chapter-item expanded "><a href="sequoia/getting-started.html"><strong aria-hidden="true">11.</strong> Getting Started</a></li><li class="chapter-item expanded "><a href="sequoia/workflows.html"><strong aria-hidden="true">12.</strong> Common Workflows</a></li><li class="chapter-item expanded "><a href="sequoia/example-workflow.html"><strong aria-hidden="true">13.</strong> Example Workflow</a></li><li class="chapter-item expanded "><a href="sequoia/best-practices.html"><strong aria-hidden="true">14.</strong> Best Practices</a></li><li class="chapter-item expanded "><a href="sequoia/performance.html"><strong aria-hidden="true">15.</strong> Performance Metrics</a></li><li class="chapter-item expanded "><a href="sequoia/overview.html"><strong aria-hidden="true">16.</strong> Storage Overview</a></li><li class="chapter-item expanded "><a href="sequoia/chunking.html"><strong aria-hidden="true">17.</strong> Smart Chunking</a></li><li class="chapter-item expanded "><a href="sequoia/manifest.html"><strong aria-hidden="true">18.</strong> Manifest Format</a></li><li class="chapter-item expanded "><a href="sequoia/merkle.html"><strong aria-hidden="true">19.</strong> Merkle DAG &amp; Proofs</a></li><li class="chapter-item expanded "><a href="sequoia/api-reference.html"><strong aria-hidden="true">20.</strong> API Reference</a></li><li class="chapter-item expanded "><a href="sequoia/troubleshooting.html"><strong aria-hidden="true">21.</strong> Troubleshooting Guide</a></li><li class="chapter-item expanded "><a href="sequoia/case-studies.html"><strong aria-hidden="true">22.</strong> Real-World Case Studies</a></li><li class="chapter-item expanded "><a href="whitepapers/sequoia-architecture.html"><strong aria-hidden="true">23.</strong> Academic Whitepaper</a></li><li class="chapter-item expanded affix "><li class="part-title">Workflows</li><li class="chapter-item expanded "><a href="workflows/lambda-workflow.html"><strong aria-hidden="true">24.</strong> LAMBDA Workflow</a></li><li class="chapter-item expanded "><a href="workflows/blast-workflow.html"><strong aria-hidden="true">25.</strong> BLAST Workflow</a></li><li class="chapter-item expanded "><a href="workflows/kraken-workflow.html"><strong aria-hidden="true">26.</strong> Kraken Workflow</a></li><li class="chapter-item expanded "><a href="workflows/diamond-workflow.html"><strong aria-hidden="true">27.</strong> Diamond Workflow</a></li><li class="chapter-item expanded "><a href="workflows/mmseqs2-workflow.html"><strong aria-hidden="true">28.</strong> MMseqs2 Workflow</a></li><li class="chapter-item expanded affix "><li class="part-title">Algorithms</li><li class="chapter-item expanded "><a href="algorithms/reduction.html"><strong aria-hidden="true">29.</strong> Reduction Algorithm</a></li><li class="chapter-item expanded "><a href="algorithms/reference-selection.html"><strong aria-hidden="true">30.</strong> Reference Selection</a></li><li class="chapter-item expanded "><a href="algorithms/delta-encoding.html"><strong aria-hidden="true">31.</strong> Delta Encoding</a></li><li class="chapter-item expanded "><a href="algorithms/alignment.html"><strong aria-hidden="true">32.</strong> Needleman-Wunsch Alignment</a></li><li class="chapter-item expanded affix "><li class="part-title">Advanced Topics</li><li class="chapter-item expanded "><a href="advanced/performance.html"><strong aria-hidden="true">33.</strong> Performance Optimization</a></li><li class="chapter-item expanded "><a href="advanced/parallel.html"><strong aria-hidden="true">34.</strong> Parallel Processing</a></li><li class="chapter-item expanded "><a href="advanced/memory.html"><strong aria-hidden="true">35.</strong> Memory Management</a></li><li class="chapter-item expanded "><a href="advanced/distributed-processing.html"><strong aria-hidden="true">36.</strong> Distributed Processing</a></li><li class="chapter-item expanded "><a href="advanced/custom-aligners.html"><strong aria-hidden="true">37.</strong> Custom Aligners</a></li><li class="chapter-item expanded affix "><li class="part-title">Benchmarks</li><li class="chapter-item expanded "><a href="benchmarks/performance.html"><strong aria-hidden="true">38.</strong> Performance Benchmarks</a></li><li class="chapter-item expanded "><a href="benchmarks/compression.html"><strong aria-hidden="true">39.</strong> Compression Rates</a></li><li class="chapter-item expanded "><a href="benchmarks/quality.html"><strong aria-hidden="true">40.</strong> Quality Metrics</a></li><li class="chapter-item expanded affix "><li class="part-title">API Reference</li><li class="chapter-item expanded "><a href="api/cli.html"><strong aria-hidden="true">41.</strong> CLI Reference</a></li><li class="chapter-item expanded "><a href="api/configuration.html"><strong aria-hidden="true">42.</strong> Configuration Reference</a></li><li class="chapter-item expanded "><a href="api/formats.html"><strong aria-hidden="true">43.</strong> File Formats</a></li><li class="chapter-item expanded affix "><li class="part-title">Development</li><li class="chapter-item expanded "><a href="development/building.html"><strong aria-hidden="true">44.</strong> Building from Source</a></li><li class="chapter-item expanded "><a href="development/contributing.html"><strong aria-hidden="true">45.</strong> Contributing</a></li><li class="chapter-item expanded "><a href="development/architecture.html"><strong aria-hidden="true">46.</strong> Architecture</a></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split("#")[0].split("?")[0];
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
