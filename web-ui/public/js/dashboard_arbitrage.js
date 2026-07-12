function getApiPrefix() {
    const path = window.location.pathname;
    if (path.startsWith('/arbitrage') || path.indexOf('/arbitrage/') !== -1 || path.indexOf('/dashboard_arbitrage') !== -1) {
        return '/arbitrage/';
    } else if (path.startsWith('/okx') || path.indexOf('/okx/') !== -1) {
        return '/okx/';
    } else if (path.startsWith('/dca') || path.indexOf('/dca/') !== -1) {
        return '/dca/';
    }
    return '/';
}
let lastPrice = 0;
let botStartTime = null;
let priceChart = null;
let balanceChart = null;
let isBalanceHistoryInitialized = false;
let latestStatus = null;
let latestArbPositions = [];

const priceChartEl = document.getElementById('priceChart');
if (priceChartEl) {
    priceChart = new Chart(priceChartEl.getContext('2d'), {
        type: 'line',
        data: {
            labels: [],
            datasets: [{
                borderColor: '#1a1a1a', // Ink Black
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
}

const balanceChartEl = document.getElementById('balanceChart');
if (balanceChartEl) {
    balanceChart = new Chart(balanceChartEl.getContext('2d'), {
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
}

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

// Filter Buttons Logic
document.querySelectorAll('.filter-btn').forEach(btn => {
    btn.addEventListener('click', function() {
        document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
        this.classList.add('active');
    });
});

// Chart Dropdown Logic
const coinSelector = document.getElementById('chart-coin-selector');
if (coinSelector) {
    coinSelector.addEventListener('change', function() {
        if (priceChart) {
            priceChart.data.labels = [];
            priceChart.data.datasets[0].data = [];
            priceChart.update();
        }
        lastPrice = 0;
    });
}

async function fetchStatus() {
    try {
        const res = await fetch(getApiPrefix() + 'api/status');
        const data = await res.json();

        if (!botStartTime) {
            botStartTime = new Date(data.start_time);
        }

        const usdt = data.simulated_balance;
        
        // Update System Resources
        const cpuEl = document.getElementById('cpu-counter');
        if (cpuEl) cpuEl.innerText = (data.sys_cpu_pct || 0).toFixed(1) + '%';
        const ramEl = document.getElementById('ram-counter');
        if (ramEl) ramEl.innerText = (data.sys_mem_mb || 0).toFixed(0) + ' MB';
        
        latestStatus = data;
        renderPipeline();
        
        // Global Corrector LED
        if (data.corrector_active) triggerLED('led-corrector', 'active-red'); else turnOffLED('led-corrector');

        // Update Price & Flashing Effect
        const coinSelector = document.getElementById('chart-coin-selector');
        const selectedCoin = coinSelector ? coinSelector.value : 'BTC';
        
        let price = 0;
        if (selectedCoin === 'BTC') price = data.current_btc_price || 96000.0; // fallback if not present
        else if (selectedCoin === 'ETH') price = data.current_eth_price || 3400.0;
        else if (selectedCoin === 'BNB') price = data.current_bnb_price || 600.0;
        else if (selectedCoin === 'SOL') price = data.current_sol_price || 150.0;
        else if (selectedCoin === 'XRP') price = data.current_xrp_price || 2.4;

        const priceEl = document.getElementById('btc-price');
        if (priceEl && price > 0) {
            priceEl.innerText = fmtUSD(price);
            if (lastPrice > 0) {
                if (price > lastPrice) priceEl.className = "price-val up";
                else if (price < lastPrice) priceEl.className = "price-val down";
            }
            lastPrice = price;

            const timeStr = new Date().toLocaleTimeString('id-ID');
            if (priceChart) {
                if (priceChart.data.labels.length === 0 || priceChart.data.datasets[0].data[priceChart.data.datasets[0].data.length - 1] !== price) {
                    priceChart.data.labels.push(timeStr);
                    priceChart.data.datasets[0].data.push(price);
                    if (priceChart.data.labels.length > 25) {
                        priceChart.data.labels.shift();
                        priceChart.data.datasets[0].data.shift();
                    }
                    priceChart.update();
                }
            }
        }
    } catch (e) {
        console.error("Error fetching status:", e);
    }
}

function renderPipeline() {
    const body = document.getElementById('pipeline-body');
    if (!body) return;

    let html = '';

    if (latestStatus) {
        const wsClass = latestStatus.ws_active ? 'active-green' : 'active-gray';
        const conclClass = latestStatus.engine_active ? 'active-blue' : 'active-gray';
        const valClass = latestStatus.engine_active ? 'active-green' : 'active-gray';
        const execClass = latestStatus.engine_active ? 'active-green' : 'active-gray';

        html += `
            <tr>
                <td style="padding: 10px; border-bottom: 1px solid var(--border-color); text-align: left; font-weight: bold; color: var(--accent-blue);">
                    Arbitrage Master Core <span style="font-size: 0.8em; font-weight: normal; color: var(--text-secondary);">(proses_altcoin.rs, validate.rs, executor.rs)</span>
                </td>
                <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light ${wsClass}" style="margin: 0 auto;"></div></td>
                <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light active-green" style="margin: 0 auto;"></div></td>
                <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light ${conclClass}" style="margin: 0 auto;"></div></td>
                <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light ${valClass}" style="margin: 0 auto;"></div></td>
                <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light ${execClass}" style="margin: 0 auto;"></div></td>
            </tr>
        `;
    }

    if (latestArbPositions && latestArbPositions.length > 0) {
        latestArbPositions.forEach(pos => {
            const wsClass = (latestStatus && latestStatus.ws_active) ? 'active-green' : 'active-gray';
            html += `
                <tr>
                    <td style="padding: 10px; border-bottom: 1px solid var(--border-color); text-align: left; font-weight: bold; color: var(--accent-blue);">
                        Arb Position - ${pos.symbol} <span style="font-size: 0.8em; font-weight: normal; color: var(--text-secondary);">(dual-leg spot/futures)</span>
                    </td>
                    <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light ${wsClass}" style="margin: 0 auto;"></div></td>
                    <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light active-green" style="margin: 0 auto;"></div></td>
                    <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light active-green" style="margin: 0 auto;"></div></td>
                    <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light active-green" style="margin: 0 auto;"></div></td>
                    <td style="padding: 10px; border-bottom: 1px solid var(--border-color);"><div class="led-light active-green" style="margin: 0 auto;"></div></td>
                </tr>
            `;
        });
    }

    if (html === '') {
        body.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-secondary);">Membaca status engine...</td></tr>`;
    } else {
        body.innerHTML = html;
    }
}

async function fetchHistory() {
    try {
        // Fetch Current Arbitrage History
        const res = await fetch(getApiPrefix() + 'api/history');
        const list = await res.json();
        
        const historyBody = document.getElementById('history-body');
        if (historyBody) {
            if (list.length === 0) {
                historyBody.innerHTML = `<tr><td colspan="5" style="text-align: center; color: var(--text-secondary);">Tidak ada transaksi arbitrage aktif</td></tr>`;
            } else {
                historyBody.innerHTML = list.map(item => {
                    const actClass = item.action.includes('OPEN') ? 'text-open-arb' : 'text-close-arb';
                    const time = new Date(item.timestamp).toLocaleString('id-ID');
                    return `
                        <tr>
                             <td>${time}</td>
                             <td class="${actClass}">${item.action}</td>
                             <td>${fmtUSD(item.price)}</td>
                             <td>${item.amount.toFixed(4)} (${fmtUSD(item.amount * item.price)})</td>
                             <td>${item.notes || '-'}</td>
                        </tr>
                    `;
                }).join('');
            }
        }
    } catch (e) {
        console.error("Error fetching arbitrage history:", e);
    }
}

async function fetchHistoryLegacy() {
    try {
        // Fetch Legacy Altcoin Trend History
        const res = await fetch(getApiPrefix() + 'api/history_legacy');
        const list = await res.json();
        
        const legacyBody = document.getElementById('history-legacy-body');
        if (legacyBody) {
            if (list.length === 0) {
                legacyBody.innerHTML = `<tr><td colspan="5" style="text-align: center; color: var(--text-secondary);">Tidak ada riwayat lama Altcoin (Trend-Following)</td></tr>`;
            } else {
                legacyBody.innerHTML = list.map(item => {
                    const actClass = item.action === 'BUY' ? 'text-buy' : 'text-sell';
                    const time = new Date(item.timestamp).toLocaleString('id-ID');
                    return `
                        <tr>
                             <td>${time}</td>
                             <td class="${actClass}">${item.action}</td>
                             <td>${fmtUSD(item.price)}</td>
                             <td>${item.amount.toFixed(4)} (${fmtUSD(item.amount * item.price)})</td>
                             <td>${item.notes || '-'}</td>
                        </tr>
                    `;
                }).join('');
            }
        }
    } catch (e) {
        console.error("Error fetching legacy history:", e);
    }
}

async function fetchCorrections() {
    try {
        const res = await fetch(getApiPrefix() + 'api/corrections');
        const list = await res.json();
        const correctionsBody = document.getElementById('corrections-body');
        if (correctionsBody) {
            if (list.length === 0) {
                correctionsBody.innerHTML = `<tr><td colspan="3" style="text-align: center; color: var(--text-secondary);">Tidak ada koreksi error saat ini</td></tr>`;
            } else {
                correctionsBody.innerHTML = list.map(item => {
                    const time = new Date(item.timestamp).toLocaleTimeString('id-ID');
                    return `
                        <tr>
                            <td>${time}</td>
                            <td class="text-fail">${item.error_type}</td>
                            <td>${item.reason}</td>
                        </tr>
                    `;
                }).join('');
            }
        }
    } catch (e) {}
}

async function fetchBalanceHistory() {
    if (isBalanceHistoryInitialized) return;
    try {
        const res = await fetch(getApiPrefix() + 'api/balance_history');
        const list = await res.json();
        if (balanceChart && list && list.length > 0) {
            const slicedList = list.slice(-150);
            balanceChart.data.labels = slicedList.map(item => {
                const cleanTime = item.timestamp.includes('.') ? (item.timestamp.split('.')[0] + 'Z') : item.timestamp;
                return new Date(cleanTime).toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' });
            });
            balanceChart.data.datasets[0].data = slicedList.map(item => item.total_value);
            balanceChart.update();
            isBalanceHistoryInitialized = true;
        }
    } catch (e) {}
}

// Helper LEDs
function triggerLED(id, activeClass) {
    const el = document.getElementById(id);
    if (el) el.className = "led-light " + activeClass;
}

function turnOffLED(id) {
    const el = document.getElementById(id);
    if (el) el.className = "led-light";
}

async function fetchJournal() {
    try {
        const res = await fetch(getApiPrefix() + 'api/logs');
        const list = await res.json();
        const box = document.getElementById('journal-box');
        if (box) {
            box.innerText = list.join('\n');
        }
    } catch (e) {}
}

async function fetchArbPositions() {
    try {
        const res = await fetch(getApiPrefix() + 'api/arb_positions');
        const list = await res.json();
        latestArbPositions = list;
        renderPipeline();
        
        const totalRemainingCash = latestStatus ? latestStatus.simulated_balance : 55134.18;
        const totalAssetValue = list.reduce((sum, pos) => sum + pos.position_size_usdt, 0.0);
        const totalEquity = totalRemainingCash + totalAssetValue;
        const totalStartingCapital = latestStatus ? latestStatus.total_equity - (latestStatus.total_funding_collected || 0.0) : 55134.18;
        const totalPnLVal = latestStatus ? latestStatus.total_funding_collected : 0.0;
        const totalPnLPct = totalStartingCapital > 0 ? (totalPnLVal / totalStartingCapital) * 100.0 : 0.0;

        // Update balances elements
        const startCapitalEl = document.getElementById('starting-capital-val');
        if (startCapitalEl) startCapitalEl.innerText = fmtUSD(totalStartingCapital);

        const usdtEl = document.getElementById('usdt-balance');
        if (usdtEl) usdtEl.innerText = fmtUSD(totalRemainingCash);

        const equityEl = document.getElementById('total-equity-val');
        if (equityEl) equityEl.innerText = fmtUSD(totalEquity);

        const pnlEl = document.getElementById('pnl-value');
        if (pnlEl) {
            const prefix = totalPnLVal >= 0 ? '+$' : '-$';
            pnlEl.innerText = `${prefix}${Math.abs(totalPnLVal).toFixed(4)} (${totalPnLVal >= 0 ? '+' : ''}${totalPnLPct.toFixed(4)}%)`;
            pnlEl.style.color = totalPnLVal >= 0 ? 'var(--accent-green)' : 'var(--accent-red)';
        }

        // Plot to balance chart real-time
        if (balanceChart && isBalanceHistoryInitialized) {
            const timeStr = new Date().toLocaleTimeString('id-ID');
            const balanceData = balanceChart.data.datasets[0].data;
            const lastPlottedBalance = balanceData[balanceData.length - 1];
            if (lastPlottedBalance === undefined || Math.abs(lastPlottedBalance - totalEquity) > 0.01) {
                balanceChart.data.labels.push(timeStr);
                balanceChart.data.datasets[0].data.push(totalEquity);
                if (balanceChart.data.labels.length > 100) {
                    balanceChart.data.labels.shift();
                    balanceChart.data.datasets[0].data.shift();
                }
                balanceChart.update();
            }
        }
        
        // Render Active Arb Position APR cards
        const mtfaContainer = document.getElementById('mtfa-container');
        if (mtfaContainer) {
            if (list.length === 0) {
                mtfaContainer.innerHTML = `<div style="text-align: center; color: var(--text-secondary); width: 100%; grid-column: 1 / -1;">Tidak ada posisi arbitrage aktif</div>`;
            } else {
                mtfaContainer.innerHTML = list.map(pos => {
                    const apr = pos.annualized_yield || 0.0;
                    const basis = ((pos.current_mark_price - pos.current_spot_price) / pos.current_spot_price) * 100.0;
                    const ledClass = apr >= 20.0 ? 'active-green' : 'active-blue';
                    const color = apr >= 20.0 ? 'var(--accent-green)' : 'var(--accent-blue)';

                    return `
                        <div style="flex: 0 0 190px; box-sizing: border-box; background: #ffffff; padding: 10px; border-radius: 4px; border: 1px solid var(--border-color);">
                            <div style="font-weight: bold; color: var(--accent-blue); margin-bottom: 8px; border-bottom: 1px dashed var(--border-color); padding-bottom: 5px;">${pos.symbol}</div>
                            <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 5px;">
                                <span style="font-size: 0.8em; color: var(--text-secondary); width: 40px;">APR</span>
                                <div class="led-light ${ledClass}"></div>
                                <span style="font-size: 0.8em; color: ${color}; font-weight: bold;">${apr.toFixed(2)}%</span>
                            </div>
                            <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 5px;">
                                <span style="font-size: 0.8em; color: var(--text-secondary); width: 40px;">Basis</span>
                                <span style="font-size: 0.8em; color: var(--text-primary); font-weight: bold;">${basis.toFixed(3)}%</span>
                            </div>
                            <div style="display: flex; align-items: center; gap: 10px;">
                                <span style="font-size: 0.8em; color: var(--text-secondary); width: 40px;">Collect</span>
                                <span style="font-size: 0.8em; color: var(--accent-green); font-weight: bold;">${pos.funding_payments_count}x</span>
                            </div>
                        </div>
                    `;
                }).join('');
            }
        }
        
        // 1. Render Card 1: Top Funding Rates
        try {
            const frRes = await fetch(getApiPrefix() + 'api/funding_rates');
            const frList = await frRes.json();
            const filteredBody = document.getElementById('altcoin-filtered-body');
            if (filteredBody) {
                if (frList.length === 0) {
                    filteredBody.innerHTML = `<tr><td colspan="3" style="text-align: center; color: var(--text-secondary);">Tidak ada data</td></tr>`;
                } else {
                    filteredBody.innerHTML = frList.slice(0, 10).map(item => {
                        const apr = item.funding_rate * 3 * 365 * 100;
                        return `
                            <tr>
                                <td style="font-weight: bold; color: var(--accent-blue);">${item.symbol}</td>
                                <td style="color: var(--accent-green); font-weight: bold;">${(item.funding_rate * 100).toFixed(4)}%</td>
                                <td style="font-weight: bold;">${apr.toFixed(2)}% APR</td>
                            </tr>
                        `;
                    }).join('');
                }
            }
        } catch (err) {
            console.error("Error fetching top funding rates:", err);
        }
        
        // 2. Render Card 2: Monitored Symbols Count
        const monitoredBody = document.getElementById('altcoin-monitored-body');
        if (monitoredBody) {
            if (list.length === 0) {
                monitoredBody.innerHTML = `<tr><td colspan="4" style="text-align: center; color: var(--text-secondary);">Menunggu posisi terbuka...</td></tr>`;
            } else {
                monitoredBody.innerHTML = list.map(pos => {
                    const basis = ((pos.current_mark_price - pos.current_spot_price) / pos.current_spot_price) * 100.0;
                    return `
                        <tr>
                            <td style="font-weight: bold;">${pos.symbol}</td>
                            <td>${fmtUSD(pos.current_spot_price)}</td>
                            <td>${fmtUSD(pos.current_mark_price)}</td>
                            <td style="color: var(--accent-blue); font-weight: bold;">${basis.toFixed(3)}%</td>
                        </tr>
                    `;
                }).join('');
            }
        }
        
        // 2.5 Fetch & Render Card 3: Funding Payment Logs
        try {
            const fundingLogRes = await fetch(getApiPrefix() + 'api/funding_log');
            const fundingLogs = await fundingLogRes.json();
            const excludedBody = document.getElementById('altcoin-excluded-body');
            if (excludedBody) {
                if (fundingLogs.length === 0) {
                    excludedBody.innerHTML = `<tr><td colspan="4" style="text-align: center; color: var(--text-secondary);">Belum ada pembayaran pendanaan terkumpul.</td></tr>`;
                } else {
                    excludedBody.innerHTML = fundingLogs.slice(0, 10).map(log => {
                        const time = new Date(log.timestamp).toLocaleTimeString('id-ID');
                        const prefix = log.payment_amount >= 0 ? '+' : '';
                        const colorClass = log.payment_amount >= 0 ? 'text-buy' : 'text-sell';
                        return `
                            <tr>
                                <td style="font-weight: bold;">${log.symbol}</td>
                                <td>${(log.funding_rate * 100).toFixed(4)}%</td>
                                <td class="${colorClass}" style="font-weight: bold;">${prefix}${fmtUSD(log.payment_amount)}</td>
                                <td style="font-weight: bold;">${time}</td>
                            </tr>
                        `;
                    }).join('');
                }
            }
        } catch (err) {
            console.error("Error fetching funding logs:", err);
        }
        
        // 3. Render Card 4: Active Arbitrage Positions Overview
        const tradingBody = document.getElementById('altcoin-trading-body');
        if (tradingBody) {
            if (list.length === 0) {
                tradingBody.innerHTML = `<tr><td colspan="4" style="text-align: center; color: var(--text-secondary);">Tidak ada posisi arbitrage aktif</td></tr>`;
            } else {
                tradingBody.innerHTML = list.map(pos => {
                    return `
                        <tr>
                            <td style="font-weight: bold;">${pos.symbol}</td>
                            <td style="color: var(--accent-green); font-weight: bold;">OPEN</td>
                            <td>${fmtUSD(pos.position_size_usdt)}</td>
                            <td>${pos.annualized_yield.toFixed(1)}% APR</td>
                        </tr>
                    `;
                }).join('');
            }
        }
        
        // 4. Render Detail Table
        const detailsBody = document.getElementById('altcoin-details-body');
        if (detailsBody) {
            if (list.length === 0) {
                detailsBody.innerHTML = `
                    <tr>
                        <td colspan="10" style="text-align: center; color: var(--text-secondary);">Tidak ada posisi aktif saat ini.</td>
                    </tr>
                `;
            } else {
                detailsBody.innerHTML = list.map(pos => {
                    const basis = ((pos.current_mark_price - pos.current_spot_price) / pos.current_spot_price) * 100.0;
                    return `
                        <tr>
                            <td style="font-weight: bold; color: var(--accent-blue);">${pos.symbol}</td>
                            <td>${fmtUSD(pos.spot_entry_price)}</td>
                            <td>${fmtUSD(pos.futures_entry_price)}</td>
                            <td>${fmtUSD(pos.current_spot_price)}</td>
                            <td>${fmtUSD(pos.current_mark_price)}</td>
                            <td style="font-weight: bold; color: ${basis >= 0 ? 'var(--accent-green)' : 'var(--accent-red)'};">${basis.toFixed(3)}%</td>
                            <td>${fmtUSD(pos.position_size_usdt)}</td>
                            <td style="font-weight: bold; color: var(--accent-green);">${fmtUSD(pos.total_funding_collected)}</td>
                            <td>${pos.funding_payments_count}x</td>
                            <td style="color: var(--accent-green); font-weight: bold;">${pos.annualized_yield.toFixed(2)}%</td>
                        </tr>
                    `;
                }).join('');
            }
        }
    } catch (e) {
        console.error("Error fetching arbitrage positions:", e);
    }
}

// Intervals
setInterval(fetchStatus, 1000);
setInterval(fetchArbPositions, 3000);
setInterval(fetchHistory, 10000);
setInterval(fetchHistoryLegacy, 10000);
setInterval(fetchCorrections, 10000);
setInterval(fetchBalanceHistory, 10000);
setInterval(fetchJournal, 10000);

// Init
fetchStatus();
fetchArbPositions();
fetchHistory();
fetchHistoryLegacy();
fetchCorrections();
fetchBalanceHistory();
fetchJournal();

const currentPath = window.location.pathname;
document.querySelectorAll('.sidebar-link').forEach(link => {
    if (link.getAttribute('href') === currentPath || (currentPath === '/' && link.getAttribute('href') === '/')) {
        link.classList.add('active');
    } else {
        link.classList.remove('active');
    }
});
