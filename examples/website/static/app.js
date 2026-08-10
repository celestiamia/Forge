// Interactive client-side features for the Forge website

function incrementCounter() {
    var counter = document.getElementById('counter');
    var count = parseInt(counter.textContent.split(': ')[1]) || 0;
    count++;
    counter.textContent = 'Count: ' + count;
}

// Fetch API status when available
function fetchStatus() {
    fetch('/api/status')
        .then(function(response) { return response.text(); })
        .then(function(data) {
            console.log('Server status:', data);
        })
        .catch(function(err) {
            console.error('Failed to fetch status:', err);
        });
}

// Initialize on page load
document.addEventListener('DOMContentLoaded', function() {
    console.log('Forge website loaded');
    if (document.getElementById('demo')) {
        fetchStatus();
    }
});