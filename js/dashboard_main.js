// js/dashboard_main.js - Unified Multi-Bot Main Dashboard script

let equityChart = null;

// Helper to format currency
function formatUSD(value) {
    if (value === undefined || value === null || isNaN(value)) return '$-';
    return '$' + parseFloat(value).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

// Helper to format timestamp
function formatTime(isoString) {
    if (!isoString) return '-';
    try {
        const date = new Date(isoString);
        return date.toLocaleString('en-US', { hour: '2-digit', minute: '2-digit', second: '2-digit', day: '2-digit', month: '2-digit' });
    } catch (e) {
        return isoString;
    }
}

// Function to update status LED
function updateLED(elementId, isActive, colorClass = 'active-green') {
    const el = document.getElementById(elementId);
    if (!el) return;
    el.className = 'led-light';
    if (isActive) {
        el.classList.add(colorClass);
    }
}

// Global data objects
let botAData = { status: null, balance: [], history: [] };
let botBData = { status: null, balance: [], history: [] };
let botCData = { status: null, balance: [], history: [] };
let botDData = { status: null, balance: [], history: [], alt_coins: [] };
let botEData = { status: null, balance: [], history: [] };

async function fetchStatusA() {
    try {
        const res = await fetch('/api/status');
        if (res.ok) botAData.status = await res.json();
    } catch (e) {
        console.error("Failed to fetch Bot A status:", e);
        botAData.status = null;
    }
}

async function fetchHistoryA() {
    try {
        const resBal = await fetch('/api/balance_history');
        if (resBal.ok) botAData.balance = await resBal.json();
    } catch (e) {
        console.error("Failed to fetch Bot A balance:", e);
        botAData.balance = [];
    }
    try {
        const resHist = await fetch('/api/history');
        if (resHist.ok) botAData.history = await resHist.json();
    } catch (e) {
        console.error("Failed to fetch Bot A history:", e);
        botAData.history = [];
    }
}

async function fetchStatusB() {
    try {
        const res = await fetch('/dca/api/status');
        if (res.ok) botBData.status = await res.json();
    } catch (e) {
        console.error("Failed to fetch Bot B status:", e);
        botBData.status = null;
    }
}

async function fetchHistoryB() {
    try {
        const resBal = await fetch('/dca/api/balance');
        if (resBal.ok) botBData.balance = await resBal.json();
    } catch (e) {
        console.error("Failed to fetch Bot B balance:", e);
        botBData.balance = [];
    }
    try {
        const resHist = await fetch('/dca/api/history');
        if (resHist.ok) botBData.history = await resHist.json();
    } catch (e) {
        console.error("Failed to fetch Bot B history:", e);
        botBData.history = [];
    }
}

async function fetchStatusC() {
    try {
        const res = await fetch('/okx/api/status');
        if (res.ok) botCData.status = await res.json();
    } catch (e) {
        console.error("Failed to fetch Bot C status:", e);
        botCData.status = null;
    }
}

async function fetchHistoryC() {
    try {
        const resBal = await fetch('/okx/api/balance_history');
        if (resBal.ok) botCData.balance = await resBal.json();
    } catch (e) {
        console.error("Failed to fetch Bot C balance:", e);
        botCData.balance = [];
    }
    try {
        const resHist = await fetch('/okx/api/history');
        if (resHist.ok) botCData.history = await resHist.json();
    } catch (e) {
        console.error("Failed to fetch Bot C history:", e);
        botCData.history = [];
    }
}

async function fetchStatusD() {
    try {
        const res = await fetch('/arbitrage/api/status');
        if (res.ok) botDData.status = await res.json();
    } catch (e) {
        console.error("Failed to fetch Bot D status:", e);
        botDData.status = null;
    }
    try {
        const resCoins = await fetch('/arbitrage/api/alt_coins');
        if (resCoins.ok) botDData.alt_coins = await resCoins.json();
    } catch (e) {
        console.error("Failed to fetch Bot D alt coins:", e);
        botDData.alt_coins = [];
    }
}

async function fetchHistoryD() {
    try {
        const resBal = await fetch('/arbitrage/api/balance_history');
        if (resBal.ok) botDData.balance = await resBal.json();
    } catch (e) {
        console.error("Failed to fetch Bot D balance:", e);
        botDData.balance = [];
    }
    try {
        const resHist = await fetch('/arbitrage/api/history');
        if (resHist.ok) botDData.history = await resHist.json();
    } catch (e) {
        console.error("Failed to fetch Bot D history:", e);
        botDData.history = [];
    }
}

async function fetchStatusE() {
    try {
        const res = await fetch('/statarb/api/status');
        if (res.ok) botEData.status = await res.json();
    } catch (e) {
        console.error("Failed to fetch Bot E status:", e);
        botEData.status = null;
    }
}

async function fetchHistoryE() {
    try {
        const resBal = await fetch('/statarb/api/balance_history');
        if (resBal.ok) botEData.balance = await resBal.json();
    } catch (e) {
        console.error("Failed to fetch Bot E balance:", e);
        botEData.balance = [];
    }
    try {
        const resHist = await fetch('/statarb/api/history');
        if (resHist.ok) botEData.history = await resHist.json();
    } catch (e) {
        console.error("Failed to fetch Bot E history:", e);
        botEData.history = [];
    }
}

function renderBotACard() {
    const s = botAData.status;
    const card = document.getElementById('card-bot-a');
    if (!card) return;

    if (!s) {
        // Offline
        updateLED('bot-a-status-led', true, 'active-red');
        const _a1=document.getElementById('bot-a-btc-price');if(_a1)_a1.innerText = 'OFFLINE';
        const _a2=document.getElementById('bot-a-equity');if(_a2)_a2.innerText = 'OFFLINE';
        return;
    }

    const totalEquity = s.simulated_balance + (s.btc_balance * s.current_btc_price);
    const pnl = totalEquity - 1000.0;
    const pnlPct = (pnl / 1000.0) * 100;

    // Calculate daily winrate (from today's history data)
    const todayStr = new Date().toDateString();
    const todaySells = ((botAData.history)||[]).filter(t => new Date(t.timestamp).toDateString() === todayStr && t.action === 'SELL');
    const winSells = todaySells.filter(t => t.notes && (t.notes.includes('+') || t.notes.includes('profit') || t.notes.includes('WIN')));
    const dailyWinrate = todaySells.length > 0 ? (winSells.length / todaySells.length * 100) : 0.0;

    const pnlColor = pnl >= 0 ? '#2ecc71' : '#e74c3c';
    const wrColor = todaySells.length > 0 ? (dailyWinrate >= 50.0 ? '#2ecc71' : '#e74c3c') : 'var(--text-secondary)';
    const pnlSign = pnl >= 0 ? '+' : '';

    updateLED('bot-a-status-led', true, 'active-green');
    const _a1=document.getElementById('bot-a-btc-price');if(_a1)_a1.innerText = formatUSD(s.current_btc_price);
    document.getElementById('bot-a-equity').innerHTML = `<span style="color: ${pnlColor}; font-weight: bold;">${formatUSD(totalEquity)}</span>`;
    (function(){var _n=document.getElementById('bot-a-usdt');if(_n)_n.innerText = formatUSD(s.simulated_balance);;})();
    (function(){var _n=document.getElementById('bot-a-btc');if(_n)_n.innerText = (s.btc_balance||0).toFixed(6);;})();
    
    document.getElementById('bot-a-winrate').innerHTML = `
        <span style="color: ${wrColor}; font-weight: bold;">${dailyWinrate.toFixed(1)}%</span> 
        (<span style="color: ${pnlColor}; font-weight: bold;">${pnlSign}${pnlPct.toFixed(2)}%</span>) 
        <span style="font-size: 0.8em; color: var(--text-secondary); display: block; margin-top: 2px;">[Daily: ${todaySells.length} sells]</span>
    `;
    (function(){var _n=document.getElementById('bot-a-regime');if(_n)_n.innerText = `${s.market_regime} (${(s.market_volatility||0).toFixed(4)}%)`;;})();

    // Pipeline
    updateLED('bot-a-led-ws', s.ws_active, 'active-green');
    updateLED('bot-a-led-conclude', s.conclude_active, 'active-green');
    updateLED('bot-a-led-validate', s.validate_active, 'active-green');
    updateLED('bot-a-led-executor', s.executor_active, 'active-green');
    updateLED('bot-a-led-corrector', s.corrector_active, 'active-green');
}

function renderBotBCard() {
    const s = botBData.status;
    const card = document.getElementById('card-bot-b');
    if (!card) return;

    if (!s) {
        updateLED('bot-b-status-led', true, 'active-red');
        const p = document.getElementById('bot-b-btc-price'); if(p) p.innerText = 'OFFLINE';
        const e = document.getElementById('bot-b-equity'); if(e) e.innerText = 'OFFLINE';
        return;
    }

    const totalEquity = s.total_equity;
    const pnl = totalEquity - 700.0;
    const pnlPct = (pnl / 700.0) * 100;

    // Calculate daily winrate (from today's history data)
    const todayStr = new Date().toDateString();
    const todaySells = botBData.history.filter(t => new Date(t.timestamp).toDateString() === todayStr && t.action === 'SELL');
    const winSells = todaySells.filter(t => t.status === 'WIN' || (t.notes && (t.notes.includes('WIN') || t.notes.includes('+'))));
    const dailyWinrate = todaySells.length > 0 ? (winSells.length / todaySells.length * 100) : 0.0;

    const pnlColor = pnl >= 0 ? '#2ecc71' : '#e74c3c';
    const wrColor = todaySells.length > 0 ? (dailyWinrate >= 50.0 ? '#2ecc71' : '#e74c3c') : 'var(--text-secondary)';
    const pnlSign = pnl >= 0 ? '+' : '';

    updateLED('bot-b-status-led', true, 'active-green');
    (function(){var _n=document.getElementById('bot-b-btc-price');if(_n)_n.innerText = formatUSD(s.current_price);;})();
    document.getElementById('bot-b-equity').innerHTML = `<span style="color: ${pnlColor}; font-weight: bold;">${formatUSD(totalEquity)}</span>`;
    (function(){var _n=document.getElementById('bot-b-usdt');if(_n)_n.innerText = formatUSD(s.simulated_balance);;})();
    (function(){var _n=document.getElementById('bot-b-btc');if(_n)_n.innerText = `${(s.btc_balance||0).toFixed(6)} (${s.layers_filled || 0}/3)`;;})();
    
    document.getElementById('bot-b-winrate').innerHTML = `
        <span style="color: ${wrColor}; font-weight: bold;">${dailyWinrate.toFixed(1)}%</span> 
        (<span style="color: ${pnlColor}; font-weight: bold;">${pnlSign}${pnlPct.toFixed(2)}%</span>) 
        <span style="font-size: 0.8em; color: var(--text-secondary); display: block; margin-top: 2px;">[Daily: ${todaySells.length} sells]</span>
    `;
    (function(){var _n=document.getElementById('bot-b-panic');if(_n)_n.innerText = s.ws_active ? 'RUNNING' : 'STOPPED';;})();

    // Pipeline
    updateLED('bot-b-led-ws', s.ws_active, 'active-green');
    updateLED('bot-b-led-conclude', s.conclude_active, 'active-green');
    updateLED('bot-b-led-validate', s.validate_active, 'active-green');
    updateLED('bot-b-led-executor', s.executor_active, 'active-green');
    updateLED('bot-b-led-panic', s.conclude_active, 'active-green');
}

function renderBotCCard() {
    const s = botCData.status;
    const card = document.getElementById('card-bot-c');
    if (!card) return;

    if (!s) {
        updateLED('bot-c-status-led', true, 'active-red');
        const p = document.getElementById('bot-c-btc-price'); if(p) p.innerText = 'OFFLINE';
        const e = document.getElementById('bot-c-equity'); if(e) e.innerText = 'OFFLINE';
        return;
    }

    // Support multi-asset grid fields
    let totalAssetsValue = 0;
    if (s.asset_balances && s.prices) {
        for (const sym in s.asset_balances) {
            totalAssetsValue += s.asset_balances[sym] * (s.prices[sym] || 0);
        }
    }
    const totalEquity = s.simulated_balance + totalAssetsValue;
    
    // Starting capital is $200.00
    const startCapital = 200.0;
    const pnl = totalEquity - startCapital;
    const pnlPct = (pnl / startCapital) * 100;

    // Calculate daily winrate (from today's history data)
    const todayStr = new Date().toDateString();
    const todaySells = botCData.history.filter(t => new Date(t.timestamp).toDateString() === todayStr && t.action === 'SELL');
    const winSells = todaySells.filter(t => t.notes && (t.notes.includes('+') || t.notes.includes('profit') || t.notes.includes('WIN')));
    const dailyWinrate = todaySells.length > 0 ? (winSells.length / todaySells.length * 100) : 0.0;

    const pnlColor = pnl >= 0 ? '#2ecc71' : '#e74c3c';
    const wrColor = todaySells.length > 0 ? (dailyWinrate >= 50.0 ? '#2ecc71' : '#e74c3c') : 'var(--text-secondary)';
    const pnlSign = pnl >= 0 ? '+' : '';

    updateLED('bot-c-status-led', true, 'active-green');
    
    // Get BTC price
    const btcPrice = s.prices ? (s.prices['BTCUSDT'] || 0) : 0;
    const btcBalance = s.asset_balances ? (s.asset_balances['BTCUSDT'] || 0) : 0;
    const btcRegime = s.market_regimes ? (s.market_regimes['BTCUSDT'] || 'GRID') : 'GRID';
    const btcVol = s.volatilities ? (s.volatilities['BTCUSDT'] || 0) : 0;

    (function(){var _n=document.getElementById('bot-c-btc-price');if(_n)_n.innerText = formatUSD(btcPrice);;})();
    document.getElementById('bot-c-equity').innerHTML = `<span style="color: ${pnlColor}; font-weight: bold;">${formatUSD(totalEquity)}</span>`;
    (function(){var _n=document.getElementById('bot-c-usdt');if(_n)_n.innerText = formatUSD(s.simulated_balance);;})();
    (function(){var _n=document.getElementById('bot-c-btc');if(_n)_n.innerText = btcBalance.toFixed(6);;})();
    
    document.getElementById('bot-c-winrate').innerHTML = `
        <span style="color: ${wrColor}; font-weight: bold;">${dailyWinrate.toFixed(1)}%</span> 
        (<span style="color: ${pnlColor}; font-weight: bold;">${pnlSign}${pnlPct.toFixed(2)}%</span>) 
        <span style="font-size: 0.8em; color: var(--text-secondary); display: block; margin-top: 2px;">[Daily: ${todaySells.length} sells]</span>
    `;
    (function(){var _n=document.getElementById('bot-c-regime');if(_n)_n.innerText = `${btcRegime} (${(btcVol * 100).toFixed(4)}%)`;;})();

    // Pipeline
    updateLED('bot-c-led-ws', s.ws_active, 'active-green');
    updateLED('bot-c-led-conclude', s.conclude_active, 'active-green');
    updateLED('bot-c-led-validate', s.validate_active, 'active-green');
    updateLED('bot-c-led-executor', s.executor_active, 'active-green');
    updateLED('bot-c-led-corrector', s.corrector_active, 'active-green');
}

function renderBotDCard() {
    const s = botDData.status;
    const card = document.getElementById('card-bot-d');
    if (!card) return;

    if (!s) {
        updateLED('bot-d-status-led', true, 'active-red');
        const p = document.getElementById('bot-d-btc-price'); if(p) p.innerText = 'OFFLINE';
        const e = document.getElementById('bot-d-equity'); if(e) e.innerText = 'OFFLINE';
        return;
    }

    // Equity & Balances: calculate from alt_coins list (realtime, consistent with individual dashboard)
    const altCoins = botDData.alt_coins || [];
    const totalRemainingCash = s.simulated_balance || 45120.18;
    const totalAssetValue = altCoins.reduce((sum, pos) => sum + pos.position_size_usdt, 0.0);
    const totalEquity = totalRemainingCash + totalAssetValue;
    const totalFunding = s.total_funding_collected || 0.0;
    const totalStartingCapital = s.total_equity - totalFunding;
    const pnlPct = totalStartingCapital > 0 ? (totalFunding / totalStartingCapital) * 100.0 : 0.0;

    const pnlColor = totalFunding >= 0 ? '#2ecc71' : '#e74c3c';
    const pnlSign = totalFunding >= 0 ? '+' : '';

    updateLED('bot-d-status-led', true, 'active-green');
    const elDPrice  = document.getElementById('bot-d-btc-price'); if(elDPrice)  elDPrice.innerText  = formatUSD(s.current_btc_price || 96000.0);
    const elDEq     = document.getElementById('bot-d-equity');    if(elDEq)     elDEq.innerHTML     = `<span style="color:${pnlColor};font-weight:bold">${formatUSD(totalEquity)}</span>`;
    const elDUsdt   = document.getElementById('bot-d-usdt');      if(elDUsdt)   elDUsdt.innerText   = formatUSD(totalRemainingCash);
    const elDBtc    = document.getElementById('bot-d-btc');       if(elDBtc)    elDBtc.innerText    = formatUSD(totalAssetValue); // Deployed Size
    const elDWr     = document.getElementById('bot-d-winrate');   if(elDWr)     elDWr.innerHTML     = `<span style="color:${pnlColor};font-weight:bold">${pnlSign}${formatUSD(totalFunding)}</span> (<span style="color:${pnlColor};font-weight:bold">${pnlSign}${pnlPct.toFixed(4)}%</span>)`;
    const elDRegime = document.getElementById('bot-d-regime');    if(elDRegime) elDRegime.innerText = `${altCoins.length} pairs`;

    updateLED('bot-d-led-ws',        s.ws_active,        'active-green');
    updateLED('bot-d-led-conclude',  s.ws_active,        'active-green'); // WS streams status
    updateLED('bot-d-led-validate',  s.engine_active,    'active-green');
    updateLED('bot-d-led-executor',  s.engine_active,    'active-green');
    updateLED('bot-d-led-corrector', s.corrector_active, 'active-red');
}

function renderBotECard() {
    const s = botEData.status;
    const card = document.getElementById('card-bot-e');
    if (!card) return;

    if (!s) {
        updateLED('bot-e-status-led', true, 'active-red');
        const p = document.getElementById('bot-e-btc-price'); if(p) p.innerText = 'OFFLINE';
        const e = document.getElementById('bot-e-equity'); if(e) e.innerText = 'OFFLINE';
        return;
    }

    const totalEquity = s.equity || 200.0;
    const usdtBalance = s.simulated_balance || 200.0;
    const pnl = totalEquity - 200.0;
    const pnlPct = (pnl / 200.0) * 100.0;
    const pnlColor = pnl >= 0 ? '#2ecc71' : '#e74c3c';
    const pnlSign = pnl >= 0 ? '+' : '';

    updateLED('bot-e-status-led', true, 'active-green');
    const elPrice = document.getElementById('bot-e-btc-price'); if(elPrice) elPrice.innerText = `${(s.current_ratio || 0).toFixed(5)} (Z: ${(s.z_score || 0).toFixed(2)})`;
    const elEq    = document.getElementById('bot-e-equity');    if(elEq)    elEq.innerHTML    = `<span style="color:${pnlColor};font-weight:bold">${formatUSD(totalEquity)}</span>`;
    const elUsdt  = document.getElementById('bot-e-usdt');      if(elUsdt)  elUsdt.innerText  = formatUSD(usdtBalance);
    const elBtc   = document.getElementById('bot-e-btc');       if(elBtc)   elBtc.innerText   = `${s.active_positions || 0} pairs`;
    const elWr    = document.getElementById('bot-e-winrate');   if(elWr)    elWr.innerHTML    = `<span style="color:${pnlColor};font-weight:bold">${pnlSign}${formatUSD(pnl)}</span> (<span style="color:${pnlColor};font-weight:bold">${pnlSign}${pnlPct.toFixed(2)}%</span>)`;
    const elRegime= document.getElementById('bot-e-regime');    if(elRegime)elRegime.innerText= `${s.market_regime || 'NORMAL'} (${(s.market_volatility || 0).toFixed(2)}%)`;

    updateLED('bot-e-led-ws',        s.ws_active,       'active-green');
    updateLED('bot-e-led-conclude',  s.conclude_active, 'active-green');
    updateLED('bot-e-led-validate',  s.validate_active, 'active-green');
    updateLED('bot-e-led-executor',  s.executor_active, 'active-green');
    updateLED('bot-e-led-corrector', s.corrector_active,'active-green');
}

let currentHistoryTab = 'all';

function setHistoryTab(tab) {
    currentHistoryTab = tab;
    // Update active tab button style
    ['all', 'bot-a', 'bot-b', 'bot-c', 'bot-d', 'bot-e'].forEach(t => {
        const btn = document.getElementById(`tab-${t}`);
        if (btn) {
            if (t === tab) {
                btn.classList.add('active');
            } else {
                btn.classList.remove('active');
            }
        }
    });
    // Filter and re-render directly on the client-side
    renderUnifiedHistory();
}
window.setHistoryTab = setHistoryTab;

function renderUnifiedHistory() {
    let merged = [];

    // Format Bot A transactions
    botAData.history.forEach(t => {
        merged.push({
            botKey: 'bot-a',
            botName: 'Bot A: Trend/Scalper',
            timestamp: new Date(t.timestamp),
            action: t.action,
            price: t.price,
            amount: t.amount,
            notes: t.notes || '-'
        });
    });

    // Format Bot B transactions
    botBData.history.forEach(t => {
        merged.push({
            botKey: 'bot-b',
            botName: 'Bot B: SmartDCA',
            timestamp: new Date(t.timestamp),
            action: t.action,
            price: t.price,
            amount: t.amount,
            notes: t.notes || `Layer ${t.layer}`
        });
    });

    // Format Bot C transactions
    botCData.history.forEach(t => {
        merged.push({
            botKey: 'bot-c',
            botName: 'Bot C: OKX Engine',
            timestamp: new Date(t.timestamp),
            action: t.action,
            price: t.price,
            amount: t.amount,
            notes: t.notes || '-'
        });
    });

    // Format Bot D transactions
    botDData.history.forEach(t => {
        merged.push({
            botKey: 'bot-d',
            botName: 'Bot D: Arbitrage Engine',
            timestamp: new Date(t.timestamp),
            action: t.action,
            price: t.price,
            amount: t.amount,
            notes: t.notes || '-'
        });
    });

    // Format Bot E transactions
    botEData.history.forEach(t => {
        merged.push({
            botKey: 'bot-e',
            botName: 'Bot E: Statistical Arbitrage',
            timestamp: new Date(t.timestamp),
            action: t.action,
            price: t.price,
            amount: t.amount,
            notes: t.notes || '-'
        });
    });

    // Sort descending
    merged.sort((a, b) => b.timestamp - a.timestamp);

    // Saring berdasarkan tab aktif
    if (currentHistoryTab !== 'all') {
        merged = merged.filter(t => t.botKey === currentHistoryTab);
    }

    const tbody = document.getElementById('unified-history-body');
    if (!tbody) return;

    if (merged.length === 0) {
        tbody.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-secondary); padding: 20px;">No transactions detected for this category.</td></tr>`;
        return;
    }

    tbody.innerHTML = '';
    merged.slice(0, 20).forEach(t => {
        const tr = document.createElement('tr');
        const actionClass = t.action === 'BUY' ? 'text-buy' : 'text-sell';
        
        tr.innerHTML = `
            <td style="font-weight: bold;">${t.botName}</td>
            <td>${formatTime(t.timestamp)}</td>
            <td class="${actionClass}">${t.action}</td>
            <td>${formatUSD(t.price)}</td>
            <td>${t.amount.toFixed(6)}</td>
            <td>${t.notes}</td>
        `;
        tbody.appendChild(tr);
    });
}

let currentResolution = 'daily'; // Default is daily

function setResolution(res) {
    currentResolution = res;
    // Update active button state
    ['daily', 'hourly', 'all'].forEach(r => {
        const btn = document.getElementById(`btn-res-${r === 'daily' ? 'daily' : r === 'hourly' ? 'hourly' : 'all'}`);
        if (btn) {
            if (r === res) {
                btn.classList.add('active');
            } else {
                btn.classList.remove('active');
            }
        }
    });
    // Rerender chart
    renderEquityChart();
}
window.setResolution = setResolution;

function aggregateData(balanceArray, resolution, isBotD = false) {
    if (!balanceArray || balanceArray.length === 0) return [];
    
    // Sort array by timestamp chronologically
    const sorted = [...balanceArray].sort((a, b) => new Date(a.timestamp) - new Date(b.timestamp));
    
    if (resolution === 'daily') {
        // Group by YYYY-MM-DD, keep the last element of each day
        const grouped = {};
        sorted.forEach(item => {
            if (!item.timestamp) return;
            const dateStr = item.timestamp.split('T')[0];
            grouped[dateStr] = item;
        });
        return Object.keys(grouped).sort().map(key => {
            const date = new Date(key + 'T12:00:00');
            let val = parseFloat(grouped[key].total_value || 0);
            if (isBotD && val > 0) {
                const cap = Math.round(val / 1000) * 1000;
                if (cap > 0) val = (val / cap) * 1000;
            }
            return {
                x: date,
                y: val,
                label: date.toLocaleDateString('en-US', { day: '2-digit', month: '2-digit' })
            };
        });
    } else if (resolution === 'hourly') {
        // Group by YYYY-MM-DD HH:00, keep the last element of each hour
        const grouped = {};
        sorted.forEach(item => {
            if (!item.timestamp) return;
            const date = new Date(item.timestamp);
            const key = date.getFullYear() + '-' + 
                        String(date.getMonth() + 1).padStart(2, '0') + '-' + 
                        String(date.getDate()).padStart(2, '0') + ' ' + 
                        String(date.getHours()).padStart(2, '0') + ':00';
            grouped[key] = item;
        });
        return Object.keys(grouped).sort().map(key => {
            const date = new Date(key);
            let val = parseFloat(grouped[key].total_value || 0);
            if (isBotD && val > 0) {
                const cap = Math.round(val / 1000) * 1000;
                if (cap > 0) val = (val / cap) * 1000;
            }
            return {
                x: date,
                y: val,
                label: date.toLocaleDateString('en-US', { day: '2-digit', month: '2-digit' }) + ' ' + date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })
            };
        });
    } else {
        // 'all' - Return all points, downsampled if needed
        let step = 1;
        if (sorted.length > 300) {
            step = Math.floor(sorted.length / 300);
        }
        const result = [];
        for (let i = 0; i < sorted.length; i += step) {
            const item = sorted[i];
            const date = new Date(item.timestamp);
            let val = parseFloat(item.total_value || 0);
            if (isBotD && val > 0) {
                const cap = Math.round(val / 1000) * 1000;
                if (cap > 0) val = (val / cap) * 1000;
            }
            result.push({
                x: date,
                y: val,
                label: date.toLocaleDateString('en-US', { day: '2-digit', month: '2-digit' }) + ' ' + date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })
            });
        }
        // Always include the last one
        if (sorted.length > 0 && (sorted.length - 1) % step !== 0) {
            const lastItem = sorted[sorted.length - 1];
            const date = new Date(lastItem.timestamp);
            let val = parseFloat(lastItem.total_value || 0);
            if (isBotD && val > 0) {
                const cap = Math.round(val / 1000) * 1000;
                if (cap > 0) val = (val / cap) * 1000;
            }
            result.push({
                x: date,
                y: val,
                label: date.toLocaleDateString('en-US', { day: '2-digit', month: '2-digit' }) + ' ' + date.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })
            });
        }
        return result;
    }
}

function renderEquityChart() {
    try {
        const dataPointsA = aggregateData(botAData.balance, currentResolution);
        const dataPointsB = aggregateData(botBData.balance, currentResolution);
        const dataPointsC = aggregateData(botCData.balance, currentResolution);
        const dataPointsD = aggregateData(botDData.balance, currentResolution, true);
        const dataPointsE = aggregateData(botEData.balance, currentResolution);

        // Build the union of all labels to align datasets
        const allLabelsMap = {};
        dataPointsA.forEach(p => { allLabelsMap[p.label] = p.x; });
        dataPointsB.forEach(p => { allLabelsMap[p.label] = p.x; });
        dataPointsC.forEach(p => { allLabelsMap[p.label] = p.x; });
        dataPointsD.forEach(p => { allLabelsMap[p.label] = p.x; });
        dataPointsE.forEach(p => { allLabelsMap[p.label] = p.x; });

        // Sort labels chronologically
        const sortedLabels = Object.keys(allLabelsMap).sort((a, b) => allLabelsMap[a] - allLabelsMap[b]);

        // Map points to aligned label list
        const mapToLabels = (points) => {
            return sortedLabels.map(label => {
                const match = points.find(p => p.label === label);
                return match ? match.y : null;
            });
        };

        const datasetA = {
            label: 'Bot A: Trend/Scalper',
            data: mapToLabels(dataPointsA),
            borderColor: '#0f4c81',
            borderWidth: 2,
            pointRadius: currentResolution === 'all' ? 0 : 2,
            fill: false,
            spanGaps: true
        };

        const datasetB = {
            label: 'Bot B: SmartDCA',
            data: mapToLabels(dataPointsB),
            borderColor: '#f1c40f',
            borderWidth: 2,
            pointRadius: currentResolution === 'all' ? 0 : 2,
            fill: false,
            spanGaps: true
        };

        const datasetC = {
            label: 'Bot C: Grid Engine',
            data: mapToLabels(dataPointsC),
            borderColor: '#2ecc71',
            borderWidth: 2,
            pointRadius: currentResolution === 'all' ? 0 : 2,
            fill: false,
            spanGaps: true
        };

        const datasetD = {
            label: 'Bot D: Arbitrage Engine',
            data: mapToLabels(dataPointsD),
            borderColor: '#9b59b6',
            borderWidth: 2,
            pointRadius: currentResolution === 'all' ? 0 : 2,
            fill: false,
            spanGaps: true
        };

        const datasetE = {
            label: 'Bot E: Statistical Arbitrage',
            data: mapToLabels(dataPointsE),
            borderColor: '#e67e22',
            borderWidth: 2,
            pointRadius: currentResolution === 'all' ? 0 : 2,
            fill: false,
            spanGaps: true
        };

        const ctx = document.getElementById('mainEquityChart');
        if (!ctx) return;

        if (equityChart) {
            equityChart.destroy();
        }

        equityChart = new Chart(ctx, {
            type: 'line',
            data: {
                labels: sortedLabels,
                datasets: [datasetA, datasetB, datasetC, datasetD, datasetE]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                scales: {
                    y: {
                        grid: { color: '#e5dfd5' },
                        ticks: {
                            color: '#1a1a1a',
                            callback: function(value) { return '$' + value; }
                        }
                    },
                    x: {
                        grid: { display: false },
                        ticks: {
                            color: '#1a1a1a',
                            maxTicksLimit: currentResolution === 'daily' ? 10 : currentResolution === 'hourly' ? 15 : 20
                        }
                    }
                },
                plugins: {
                    legend: {
                        position: 'top',
                        labels: { font: { family: 'Georgia' }, color: '#1a1a1a' }
                    }
                }
            }
        });

        const chartSyncTimeEl = document.getElementById('chart-sync-time');
        if (chartSyncTimeEl) {
            chartSyncTimeEl.innerText = `Last synced: ${new Date().toLocaleTimeString('en-US')}`;
        }
    } catch (e) {
        console.error("Failed to render chart:", e);
        const chartSyncTimeEl = document.getElementById('chart-sync-time');
        if (chartSyncTimeEl) {
            chartSyncTimeEl.innerText = `Failed to update chart: ${e.message}`;
        }
    }
}

async function updateRealtimeStatus() {
    fetchStatusA().then(renderBotACard);
    fetchStatusB().then(renderBotBCard);
    fetchStatusC().then(renderBotCCard);
    fetchStatusD().then(renderBotDCard);
    fetchStatusE().then(renderBotECard);
}

async function updateHistoryAndCharts() {
    const pA = fetchHistoryA();
    const pB = fetchHistoryB();
    const pC = fetchHistoryC();
    const pD = fetchHistoryD();
    const pE = fetchHistoryE();

    Promise.all([pA, pB, pC, pD, pE]).then(() => {
        renderUnifiedHistory();
        renderEquityChart();
    });
}

// Initial update & Interval configuration
window.addEventListener('load', () => {
    // 1. Ambil status realtime & historis saat pertama kali load secara paralel non-blocking
    updateRealtimeStatus();
    updateHistoryAndCharts();

    // 2. Set interval untuk status realtime (setiap 3 detik)
    setInterval(updateRealtimeStatus, 3000);

    // 3. Set interval untuk data history & grafik yang berat (setiap 30 detik)
    setInterval(updateHistoryAndCharts, 30000);
});
