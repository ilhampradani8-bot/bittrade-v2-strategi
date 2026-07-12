let lastPrice = 0;
let botStartTime = null;
let isBalanceHistoryInitialized = false;
const apiBase = window.location.protocol === 'file:' ? 'https://tradingsafe.mijdigital.my/dca/' : '';

// Initialize Equity Chart
const ctxEquity = document.getElementById('equityChart').getContext('2d');
const equityChart = new Chart(ctxEquity, {
    type: 'line',
    data: {
        labels: [],
        datasets: [{
            borderColor: '#0f4c81', // Ink Blue
            borderWidth: 1.8,
            data: [],
            fill: false,
            tension: 0.1,
            pointRadius: 0
        }]
    },
    options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: { display: false } },
        scales: {
            x: { ticks: { color: '#1a1a1a' }, grid: { color: '#e5dfd5' } },
            y: { ticks: { color: '#1a1a1a' }, grid: { color: '#e5dfd5' } }
        }
    }
});

const fmtUSD = (val) => new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(val);

// Update Uptime
setInterval(() => {
    if (!botStartTime) return;
    const now = new Date();
    const diffMs = now - botStartTime;
    const diffSec = Math.floor(diffMs / 1000);
    const hours = String(Math.floor(diffSec / 3600)).padStart(2, '0');
    const minutes = String(Math.floor((diffSec % 3600) / 60)).padStart(2, '0');
    const seconds = String(diffSec % 60).padStart(2, '0');
    
    const uptimeEl = document.getElementById('uptime-counter');
    if (uptimeEl) uptimeEl.innerText = `${hours}:${minutes}:${seconds}`;
}, 1000);

async function fetchStatus() {
    try {
        const res = await fetch(`${apiBase}api/status`);
        const data = await res.json();

        if (!botStartTime) {
            botStartTime = new Date(data.start_time);
        }

        // Price flashing effect
        const priceEl = document.getElementById('btc-price-val');
        if (priceEl) {
            priceEl.innerText = fmtUSD(data.current_price);
            if (lastPrice > 0 && data.current_price !== lastPrice) {
                priceEl.classList.add('flashing');
                setTimeout(() => priceEl.classList.remove('flashing'), 500);
            }
            lastPrice = data.current_price;
        }

        // Primary Metrics
        document.getElementById('equity-val').innerText = fmtUSD(data.total_equity);
        document.getElementById('balance-val').innerText = fmtUSD(data.simulated_balance);
        document.getElementById('btc-val').innerText = data.btc_balance.toFixed(6);
        document.getElementById('btc-value-usd').innerText = fmtUSD(data.btc_balance * data.current_price) + " USD";
        
        // Layers Metric
        document.getElementById('layers-val').innerText = `${data.layers_filled} / 3`;
        let dots = "○○○";
        if (data.layers_filled === 1) dots = "●○○";
        else if (data.layers_filled === 2) dots = "●●○";
        else if (data.layers_filled === 3) dots = "●●●";
        document.getElementById('layer-dots').innerText = dots;

        // Active Position Details
        document.getElementById('cycle-id').innerText = `#${data.current_cycle_id}`;
        document.getElementById('avg-entry-val').innerText = data.avg_entry_price > 0 ? fmtUSD(data.avg_entry_price) : "-";
        document.getElementById('hwm-val').innerText = data.cycle_high_water_mark > 0 ? fmtUSD(data.cycle_high_water_mark) : "-";

        // PNL Display
        const pnlValEl = document.getElementById('pnl-pct-val');
        if (data.avg_entry_price > 0 && data.btc_balance > 0) {
            const prefix = data.current_pnl_pct >= 0 ? '+' : '';
            pnlValEl.innerText = `${prefix}${data.current_pnl_pct.toFixed(2)}%`;
            if (data.current_pnl_pct > 0) {
                pnlValEl.style.color = 'var(--accent-green)';
            } else if (data.current_pnl_pct < 0) {
                pnlValEl.style.color = 'var(--accent-red)';
            } else {
                pnlValEl.style.color = 'var(--text-primary)';
            }
        } else {
            pnlValEl.innerText = "0.00%";
            pnlValEl.style.color = 'var(--text-secondary)';
        }

        // Header Metrics
        const winrateEl = document.getElementById('winrate-counter');
        if (winrateEl) winrateEl.innerText = `${data.winrate.toFixed(1)}%`;
        const cpuEl = document.getElementById('cpu-counter');
        if (cpuEl) cpuEl.innerText = `${data.sys_cpu_pct.toFixed(1)}%`;
        const ramEl = document.getElementById('ram-counter');
        if (ramEl) ramEl.innerText = `${data.sys_mem_mb.toFixed(0)} MB`;

        // LED Indicators
        setLED('led-ws', data.ws_active);
        setLED('led-conclude', data.conclude_active);
        setLED('led-validate', data.validate_active);
        setLED('led-executor', data.executor_active);

        // Plot ke balance/equity chart secara real-time
        const currentBalance = data.total_equity;
        const balanceData = equityChart.data.datasets[0].data;
        const lastPlottedBalance = balanceData[balanceData.length - 1];
        if (lastPlottedBalance === undefined || Math.abs(lastPlottedBalance - currentBalance) > 0.01) {
            const timeStr = new Date().toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' });
            equityChart.data.labels.push(timeStr);
            equityChart.data.datasets[0].data.push(currentBalance);
            if (equityChart.data.labels.length > 100) {
                equityChart.data.labels.shift();
                equityChart.data.datasets[0].data.shift();
            }
            equityChart.update();
        }

    } catch (e) {
        console.error("Gagal fetch status API:", e);
    }
}

function setLED(id, active) {
    const el = document.getElementById(id);
    if (!el) return;
    if (active) {
        el.className = "led-light active-green";
    } else {
        el.className = "led-light active-red";
    }
}

async function fetchBalanceHistory() {
    if (isBalanceHistoryInitialized) return;
    try {
        const res = await fetch(`${apiBase}api/balance`);
        const data = await res.json();
        
        const labels = data.map(item => {
            const d = new Date(item.timestamp);
            return d.toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' });
        });
        const values = data.map(item => item.total_value);

        equityChart.data.labels = labels;
        equityChart.data.datasets[0].data = values;
        equityChart.update();
        isBalanceHistoryInitialized = true;
    } catch (e) {
        console.error("Gagal fetch balance history:", e);
    }
}

async function fetchCycles() {
    try {
        const res = await fetch(`${apiBase}api/cycles`);
        const data = await res.json();
        const tbody = document.getElementById('cycles-table-body');
        if (!tbody) return;

        if (data.length === 0) {
            tbody.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-secondary);">Belum ada riwayat siklus selesai</td></tr>`;
            return;
        }

        tbody.innerHTML = data.map(cycle => {
            const pnlClass = cycle.status === 'WIN' ? 'badge-win' : 'badge-loss';
            const prefix = (cycle.net_pnl || 0) >= 0 ? '+' : '';
            const pnlPctPrefix = (cycle.pnl_pct || 0) >= 0 ? '+' : '';
            return `
                <tr>
                    <td>#${cycle.cycle_id}</td>
                    <td>${fmtUSD(cycle.avg_entry_price || 0)}</td>
                    <td>${fmtUSD(cycle.exit_price || 0)}</td>
                    <td class="${pnlClass}">${prefix}${fmtUSD(cycle.net_pnl || 0)}</td>
                    <td class="${pnlClass}">${pnlPctPrefix}${(cycle.pnl_pct || 0).toFixed(2)}%</td>
                    <td class="${pnlClass}">${cycle.status || '-'}</td>
                </tr>
            `;
        }).join('');
    } catch (e) {
        console.error("Gagal fetch cycles:", e);
    }
}

async function fetchHistory() {
    try {
        const res = await fetch(`${apiBase}api/history`);
        const data = await res.json();
        const tbody = document.getElementById('history-table-body');
        if (!tbody) return;

        if (data.length === 0) {
            tbody.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-secondary);">Belum ada transaksi</td></tr>`;
            return;
        }

        tbody.innerHTML = data.map(trade => {
            const date = new Date(trade.timestamp).toLocaleTimeString('id-ID');
            const actionClass = trade.action === 'BUY' ? 'badge-win' : 'badge-loss';
            const layerStr = trade.layer ? `L${trade.layer}` : '-';
            return `
                <tr>
                    <td>${date}</td>
                    <td>#${trade.cycle_id}</td>
                    <td class="${actionClass}">${trade.action}</td>
                    <td>${layerStr}</td>
                    <td>${fmtUSD(trade.price)}</td>
                    <td>${trade.amount.toFixed(6)}</td>
                </tr>
            `;
        }).join('');
    } catch (e) {
        console.error("Gagal fetch history:", e);
    }
}

async function fetchLogs() {
    try {
        const res = await fetch(`${apiBase}api/logs`);
        const logs = await res.json();
        const logBox = document.getElementById('log-box');
        if (!logBox) return;

        logBox.innerText = logs.join('\n');
        logBox.scrollTop = logBox.scrollHeight;
    } catch (e) {
        console.error("Gagal fetch logs:", e);
    }
}

async function triggerManualSell() {
    if (!confirm("Apakah Anda yakin ingin melakukan Emergency Manual Sell untuk menutup siklus saat ini?")) {
        return;
    }

    try {
        const res = await fetch('api/manual_sell', { method: 'POST' });
        if (res.ok) {
            alert("Emergency Manual Sell berhasil dieksekusi!");
            fetchStatus();
            fetchHistory();
        } else {
            const text = await res.text();
            alert("Gagal melakukan manual sell: " + text);
        }
    } catch (e) {
        alert("Error menghubungi server: " + e);
    }
}

// Refresh Loop intervals
setInterval(fetchStatus, 1000);
setInterval(fetchLogs, 2000);
setInterval(fetchHistory, 10000);
setInterval(fetchCycles, 10000);
setInterval(fetchBalanceHistory, 15000);

// Init loads
fetchStatus();
fetchLogs();
fetchHistory();
fetchCycles();
fetchBalanceHistory();
