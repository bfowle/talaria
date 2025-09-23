// Interactive report functionality
console.log("Report loaded");

function exportToPDF() {
    window.print();
}

function exportToCSV() {
    // Extract table data and convert to CSV
    const tables = document.querySelectorAll('table');
    let csv = '';
    tables.forEach(table => {
        const rows = table.querySelectorAll('tr');
        rows.forEach(row => {
            const cells = row.querySelectorAll('td, th');
            const rowData = Array.from(cells).map(cell => cell.textContent).join(',');
            csv += rowData + '\n';
        });
    });

    const blob = new Blob([csv], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'report.csv';
    a.click();
}