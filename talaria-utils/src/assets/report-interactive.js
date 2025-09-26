// Interactive functionality for Talaria reports
(function() {
    'use strict';

    // Toggle visibility of sections
    document.querySelectorAll('.section-header').forEach(header => {
        header.addEventListener('click', function() {
            const content = this.nextElementSibling;
            if (content.style.display === 'none') {
                content.style.display = 'block';
                this.classList.add('expanded');
            } else {
                content.style.display = 'none';
                this.classList.remove('expanded');
            }
        });
    });

    // Filter table rows
    const filterInput = document.getElementById('filter-input');
    if (filterInput) {
        filterInput.addEventListener('keyup', function() {
            const filter = this.value.toLowerCase();
            const rows = document.querySelectorAll('tbody tr');

            rows.forEach(row => {
                const text = row.textContent.toLowerCase();
                row.style.display = text.includes(filter) ? '' : 'none';
            });
        });
    }

    // Sort table columns
    document.querySelectorAll('th[data-sortable]').forEach(header => {
        header.addEventListener('click', function() {
            const table = this.closest('table');
            const tbody = table.querySelector('tbody');
            const rows = Array.from(tbody.querySelectorAll('tr'));
            const columnIndex = Array.from(this.parentNode.children).indexOf(this);
            const isNumeric = this.dataset.type === 'number';

            rows.sort((a, b) => {
                const aVal = a.children[columnIndex].textContent;
                const bVal = b.children[columnIndex].textContent;

                if (isNumeric) {
                    return parseFloat(aVal) - parseFloat(bVal);
                }
                return aVal.localeCompare(bVal);
            });

            if (this.classList.contains('sorted-asc')) {
                rows.reverse();
                this.classList.remove('sorted-asc');
                this.classList.add('sorted-desc');
            } else {
                this.classList.remove('sorted-desc');
                this.classList.add('sorted-asc');
            }

            tbody.innerHTML = '';
            rows.forEach(row => tbody.appendChild(row));
        });
    });

    // Copy to clipboard
    document.querySelectorAll('.copy-btn').forEach(btn => {
        btn.addEventListener('click', function() {
            const text = this.dataset.copy;
            navigator.clipboard.writeText(text).then(() => {
                const original = this.textContent;
                this.textContent = 'Copied!';
                setTimeout(() => this.textContent = original, 2000);
            });
        });
    });
})();