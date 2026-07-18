function getApiPrefix() {
    const path = window.location.pathname;
    if (path.startsWith('/alt') || path.indexOf('/alt/') !== -1) {
        return '/alt/';
    } else if (path.startsWith('/okx') || path.indexOf('/okx/') !== -1) {
        return '/okx/';
    } else if (path.startsWith('/grid') || path.indexOf('/grid/') !== -1) {
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

const priceChartEl = document.getElementById('priceChart');
if (priceChartEl) {
    priceChart = new Chart(priceChartEl.getContext('2d'), {
        type: 'line',
        data: {
            labels: [],
            datasets: [{
                borderColor: '#1a1a1a', 
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
                borderColor: '#0f4c81', 
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

document.querySelectorAll('.filter-btn').forEach(btn => {
    btn.addEventListener('click', function() {
        document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
        this.classList.add('active');
    });
});

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
        
        let totalAssetsValue = 0;
        if(data.asset_balances && data.prices) {
             for (const sym in data.asset_balances) {
                 totalAssetsValue += data.asset_balances[sym] * (data.prices[sym] || 0);
             }
        }

        const totalEquity = usdt + totalAssetsValue;
        
        const usdtEl = document.getElementById('usdt-balance');
        if (usdtEl) usdtEl.innerText = fmtUSD(usdt);
        const totalEquityEl = document.getElementById('total-equity-val');
        if (totalEquityEl) totalEquityEl.innerText = fmtUSD(totalEquity);
        
        const selectedCoin = coinSelector ? coinSelector.value : 'BTCUSDT';
        
        const currentBal = data.asset_balances ? (data.asset_balances[selectedCoin] || 0) : 0;
        const currentPrice = data.prices ? (data.prices[selectedCoin] || 0) : 0;
        const currentAssetValue = currentBal * currentPrice;

        const btcBalEl = document.getElementById('btc-balance');
        if (btcBalEl) {
             btcBalEl.innerText = currentBal.toFixed(4) + ' ' + selectedCoin.replace('USDT','');
             btcBalEl.previousElementSibling.innerText = "Saldo Aset " + selectedCoin;
        }
        
        const btcValEl = document.getElementById('btc-value-display');
        if (btcValEl) {
             btcValEl.innerText = fmtUSD(currentAssetValue);
             btcValEl.previousElementSibling.innerText = "Nilai " + selectedCoin + " Saat Ini";
        }
        
        const winrateEl = document.getElementById('winrate-counter');
        if (winrateEl) winrateEl.innerText = data.winrate.toFixed(1) + '%';
        
        const cpuEl = document.getElementById('cpu-counter');
        if (cpuEl) cpuEl.innerText = (data.sys_cpu_pct || 0).toFixed(1) + '%';
        const ramEl = document.getElementById('ram-counter');
        if (ramEl) ramEl.innerText = (data.sys_mem_mb || 0).toFixed(0) + ' MB';
        
        const whaleBadge = document.getElementById('whale-badge');
        const whaleStatus = document.getElementById('whale-status');
        if (whaleBadge && whaleStatus) {
            if (data.whale_detected) {
                whaleBadge.style.borderColor = 'var(--accent-green)';
                whaleBadge.style.color = 'var(--accent-green)';
                whaleBadge.style.boxShadow = '0 0 10px var(--accent-green)';
                whaleStatus.innerText = 'DETECTED! 🐋';
            } else {
                whaleBadge.style.borderColor = '#3b3b3b';
                whaleBadge.style.color = '#888';
                whaleBadge.style.boxShadow = 'none';
                whaleStatus.innerText = 'SLEEPING';
            }
        }

        const regime = data.market_regimes ? (data.market_regimes[selectedCoin] || 'GRID') : 'GRID';
        const marketRegimeEl = document.getElementById('market-regime');
        if (marketRegimeEl) marketRegimeEl.innerText = regime;
        
        const vol = data.volatilities ? (data.volatilities[selectedCoin] || 0) : 0;
        const marketVolatilityEl = document.getElementById('market-volatility');
        if (marketVolatilityEl) marketVolatilityEl.innerText = vol.toFixed(3) + "%";

        try {
            const setRegime = (id, rgm) => {
                const rEl = document.getElementById('led-regime-' + id);
                const rTxt = document.getElementById('regime-text-' + id);
                if(rEl && rTxt) {
                    rEl.className = 'led-light';
                    rTxt.innerText = rgm || 'GRID';
                    if (rgm === 'BULLISH' || rgm === 'GRID') {
                        rEl.classList.add('active-green');
                        rTxt.style.color = 'var(--accent-green)';
                    } else if (rgm === 'SIDEWAYS') {
                        rEl.classList.add('active-yellow');
                        rTxt.style.color = 'var(--accent-yellow)';
                    } else if (rgm === 'BEARISH' || rgm === 'DUMP_PROTECTION' || rgm === 'STOP_LOSS') {
                        rEl.classList.add('active-red');
                        rTxt.style.color = 'var(--accent-red)';
                    }
                }
            };
            
            if (data.market_regimes) {
                for (const sym in data.market_regimes) {
                    setRegime(sym, data.market_regimes[sym]);
                }
            }

            if (data.ws_active) {
                triggerLED('led-ws-btc', 'active-green');
            } else {
                turnOffLED('led-ws-btc');
            }
            if (data.conclude_active) triggerLED('led-conclude-btc', 'active-blue'); else turnOffLED('led-conclude-btc');
            if (data.validate_active) triggerLED('led-validate-btc', 'active-green'); else turnOffLED('led-validate-btc');
            if (data.executor_active) triggerLED('led-executor-btc', 'active-green'); else turnOffLED('led-executor-btc');
            
            if (data.corrector_active) triggerLED('led-corrector', 'active-red'); else turnOffLED('led-corrector');
            if (data.conclude_active) triggerLED('led-ai-btc', 'active-blue'); else turnOffLED('led-ai-btc');
        } catch(e) {
            console.error("Error updating LEDs:", e);
        }
        
        const pnlAbsolute = totalEquity - 200.0;
        const pnlPercentage = (pnlAbsolute / 200.0) * 100.0;
        const pnlEl = document.getElementById('pnl-value');
        if (pnlEl) {
            const prefix = pnlAbsolute >= 0 ? '+$' : '-$';
            pnlEl.innerText = `${prefix}${Math.abs(pnlAbsolute).toFixed(2)} (${pnlPercentage.toFixed(2)}%)`;
            pnlEl.style.color = pnlAbsolute >= 0 ? 'var(--accent-green)' : 'var(--accent-red)';
        }

        const priceEl = document.getElementById('btc-price'); 
        if (priceEl && currentPrice > 0) {
            priceEl.innerText = fmtUSD(currentPrice);
            
            if (lastPrice > 0) {
                if (currentPrice > lastPrice) {
                    priceEl.className = "price-val up";
                } else if (currentPrice < lastPrice) {
                    priceEl.className = "price-val down";
                }
            }
            lastPrice = currentPrice;

            const timeStr = new Date().toLocaleTimeString('id-ID');
            if (priceChart) {
                if (priceChart.data.labels.length === 0 || priceChart.data.datasets[0].data[priceChart.data.datasets[0].data.length - 1] !== currentPrice) {
                    priceChart.data.labels.push(timeStr);
                    priceChart.data.datasets[0].data.push(currentPrice);
                    if (priceChart.data.labels.length > 25) {
                        priceChart.data.labels.shift();
                        priceChart.data.datasets[0].data.shift();
                    }
                    priceChart.update();
                }
            }

            if (balanceChart) {
                const currentBalance = totalEquity;
                const balanceData = balanceChart.data.datasets[0].data;
                const lastPlottedBalance = balanceData[balanceData.length - 1];
                if (lastPlottedBalance === undefined || Math.abs(lastPlottedBalance - currentBalance) > 0.01) {
                    balanceChart.data.labels.push(timeStr);
                    balanceChart.data.datasets[0].data.push(currentBalance);
                    if (balanceChart.data.labels.length > 100) {
                        balanceChart.data.labels.shift();
                        balanceChart.data.datasets[0].data.shift();
                    }
                    balanceChart.update();
                }
            }
        } else if (priceEl) {
            priceEl.innerText = fmtUSD(currentPrice);
        } else {
            turnOffLED('led-ws-btc');
        }

    } catch (e) {
        turnOffLED('led-ws-btc');
    }
}

async function fetchLogsAndSystemState() {}

async function fetchHistory() {
    try {
        const res = await fetch(getApiPrefix() + 'api/history');
        const list = await res.json();
        
        let winCount = 0;
        let lossCount = 0;

        const historyBody = document.getElementById('history-body');
        if (historyBody) {
            historyBody.innerHTML = list.map(item => {
                const actClass = item.action === 'BUY' ? 'text-buy' : 'text-sell';
                const time = new Date(item.timestamp).toLocaleTimeString('id-ID');
                const notesVal = item.notes || '-';
                let formattedNotes = notesVal;
                let rowStyle = '';
                
                if (notesVal.includes('P&L: $+')) {
                    winCount++;
                    formattedNotes = notesVal.replace(/(P&L:\s*\+\$[0-9.-]+)/g, '<span class="text-buy" style="font-weight: bold;">$1</span>');
                    rowStyle = 'background: rgba(63, 185, 80, 0.05);';
                } else if (notesVal.includes('P&L: $-')) {
                    lossCount++;
                    formattedNotes = notesVal.replace(/(P&L:\s*-\$[0-9.-]+)/g, '<span class="text-sell" style="font-weight: bold;">$1</span>');
                    rowStyle = 'background: rgba(248, 81, 73, 0.15);';
                }
                
                return `
                    <tr style="${rowStyle}">
                         <td>${time}</td>
                         <td class="${actClass}">${item.action}</td>
                         <td>${fmtUSD(item.price)}</td>
                         <td>${item.amount.toFixed(6)} ${item.symbol.replace('USDT','')} (${fmtUSD(item.amount * item.price)})</td>
                         <td>${formattedNotes}</td>
                    </tr>
                `;
            }).join('');
        }
        
        const historyTitle = document.getElementById('history-title');
        if(historyTitle) {
            historyTitle.innerText = `Riwayat Eksekusi (grid_trading_history) | Wins: ${winCount} | Losses: ${lossCount}`;
        }
    } catch (e) {}
}

async function fetchCorrections() {
    try {
        const res = await fetch(getApiPrefix() + 'api/corrections');
        const list = await res.json();
        const correctionsBody = document.getElementById('corrections-body');
        if (correctionsBody) {
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
    } catch (e) {}
}

async function fetchBalanceHistory() {
    if (isBalanceHistoryInitialized) return;
    try {
        const res = await fetch(getApiPrefix() + 'api/balance_history');
        const list = await res.json();
        
        if (balanceChart) {
            balanceChart.data.labels = list.map(item => new Date(item.timestamp).toLocaleTimeString('id-ID'));
            balanceChart.data.datasets[0].data = list.map(item => item.total_value);
            balanceChart.update();
            isBalanceHistoryInitialized = true;
        }
    } catch (e) {}
}

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
        const res = await fetch(getApiPrefix() + 'api/journal');
        const text = await res.text();
        const box = document.getElementById('journal-box');
        if (box) box.innerText = text;
    } catch (e) {}
}

async function fetchPositions() {
    try {
        const res = await fetch(getApiPrefix() + 'api/grid_positions');
        const list = await res.json();
        const positionsBody = document.getElementById('positions-body');
        if (positionsBody) {
            if (!list || list.length === 0) {
                positionsBody.innerHTML = `
                    <tr>
                        <td colspan="6" style="text-align: center; color: var(--text-secondary);">Tidak ada posisi aktif yang terbuka.</td>
                    </tr>
                `;
                return;
            }
            positionsBody.innerHTML = list.map(item => {
                const time = new Date(item.opened_at).toLocaleTimeString('id-ID');
                const val = item.amount * item.buy_price;
                return `
                    <tr>
                        <td>${item.id}</td>
                        <td>${fmtUSD(item.buy_price)}</td>
                        <td>${fmtUSD(item.high_water_mark)}</td>
                        <td>${item.amount.toFixed(6)} ${item.symbol.replace('USDT','')}</td>
                        <td>${fmtUSD(val)}</td>
                        <td>${time}</td>
                    </tr>
                `;
            }).join('');
        }
    } catch (e) {
        console.error("Error fetching positions:", e);
    }
}

setInterval(fetchStatus, 1000);
setInterval(fetchLogsAndSystemState, 2000);
setInterval(fetchHistory, 10000);
setInterval(fetchCorrections, 10000);
setInterval(fetchBalanceHistory, 10000);
setInterval(fetchJournal, 15000);
setInterval(fetchPositions, 10000);

fetchStatus();
fetchLogsAndSystemState();
fetchHistory();
fetchCorrections();
fetchBalanceHistory();
fetchJournal();
fetchPositions();

const currentPath = window.location.pathname;
document.querySelectorAll('.sidebar-link').forEach(link => {
    if (link.getAttribute('href') === currentPath || (currentPath === '/' && link.getAttribute('href') === '/')) {
        link.classList.add('active');
    } else {
        link.classList.remove('active');
    }
});
