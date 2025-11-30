const https = require('https');

const CONTRACT_ADDRESS = '0x5ebfbefb1e88391efb022d5d33302f50a46bf4f3';
const API_BASE = 'api.gnosisscan.io';
// Note: API key not required for Blockscout API (used in v2 script)

// Helper function to make API requests
function apiRequest(url) {
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      let data = '';
      res.on('data', (chunk) => data += chunk);
      res.on('end', () => {
        try {
          resolve(JSON.parse(data));
        } catch (e) {
          reject(e);
        }
      });
    }).on('error', reject);
  });
}

// Get week number and year from timestamp
function getWeekKey(timestamp) {
  const date = new Date(timestamp * 1000);
  const firstDayOfYear = new Date(date.getFullYear(), 0, 1);
  const pastDaysOfYear = (date - firstDayOfYear) / 86400000;
  const weekNumber = Math.ceil((pastDaysOfYear + firstDayOfYear.getDay() + 1) / 7);
  return `${date.getFullYear()}-W${String(weekNumber).padStart(2, '0')}`;
}

// Get date 6 months ago
function getSixMonthsAgo() {
  const date = new Date();
  date.setMonth(date.getMonth() - 6);
  return Math.floor(date.getTime() / 1000);
}

async function fetchTransactions() {
  console.log('Fetching transactions for address:', CONTRACT_ADDRESS);
  console.log('This may take a moment...\n');

  const sixMonthsAgo = getSixMonthsAgo();
  const allTransactions = [];
  let page = 1;
  const pageSize = 10000;

  try {
    // Fetch normal transactions
    console.log('Fetching normal transactions...');
    while (true) {
      const url = `https://${API_BASE}/api?module=account&action=txlist&address=${CONTRACT_ADDRESS}&startblock=0&endblock=99999999&page=${page}&offset=${pageSize}&sort=desc`;

      console.log(`  Page ${page}...`);
      const response = await apiRequest(url);

      if (response.status !== '1') {
        console.log('  API response:', response.message || response.result);
        break;
      }

      if (!response.result || response.result.length === 0) {
        break;
      }

      allTransactions.push(...response.result);

      // Check if we've gone back far enough
      const oldestTx = response.result[response.result.length - 1];
      if (parseInt(oldestTx.timeStamp) < sixMonthsAgo) {
        break;
      }

      page++;

      // Add a small delay to avoid rate limiting
      await new Promise(resolve => setTimeout(resolve, 200));
    }

    // Also fetch internal transactions (contract interactions)
    console.log('\nFetching internal transactions...');
    page = 1;
    while (true) {
      const url = `https://${API_BASE}/api?module=account&action=txlistinternal&address=${CONTRACT_ADDRESS}&startblock=0&endblock=99999999&page=${page}&offset=${pageSize}&sort=desc`;

      console.log(`  Page ${page}...`);
      const response = await apiRequest(url);

      if (response.status !== '1') {
        console.log('  API response:', response.message || response.result);
        break;
      }

      if (!response.result || response.result.length === 0) {
        break;
      }

      allTransactions.push(...response.result);

      // Check if we've gone back far enough
      const oldestTx = response.result[response.result.length - 1];
      if (parseInt(oldestTx.timeStamp) < sixMonthsAgo) {
        break;
      }

      page++;
      await new Promise(resolve => setTimeout(resolve, 200));
    }

    console.log(`\nTotal transactions fetched: ${allTransactions.length}`);
    return allTransactions;

  } catch (error) {
    console.error('Error fetching transactions:', error.message);
    throw error;
  }
}

async function analyzeTransactions() {
  const transactions = await fetchTransactions();
  const sixMonthsAgo = getSixMonthsAgo();

  // Filter transactions from last 6 months
  const recentTxs = transactions.filter(tx => parseInt(tx.timeStamp) >= sixMonthsAgo);

  console.log(`\nTransactions in last 6 months: ${recentTxs.length}`);

  // Filter for "create batch" transactions
  // This looks for transactions with input data and specific method signatures
  const batchTxs = recentTxs.filter(tx => {
    const input = tx.input.toLowerCase();
    // Common batch-related method signatures:
    // - createBatch / createBatchTransaction
    // - multiSend
    // - execTransaction (Safe/Gnosis Safe)
    return input.length > 10 && (
      input.includes('batch') ||
      tx.functionName?.toLowerCase().includes('batch') ||
      tx.methodId === '0x8d80ff0a' || // multiSend
      tx.methodId === '0x6a761202'    // execTransaction
    );
  });

  console.log(`Batch-related transactions found: ${batchTxs.length}\n`);

  // Group by week
  const weeklyStats = {};

  batchTxs.forEach(tx => {
    const weekKey = getWeekKey(parseInt(tx.timeStamp));
    if (!weeklyStats[weekKey]) {
      weeklyStats[weekKey] = {
        count: 0,
        transactions: [],
        totalGasUsed: 0,
        totalValue: 0
      };
    }

    weeklyStats[weekKey].count++;
    weeklyStats[weekKey].transactions.push(tx.hash);
    weeklyStats[weekKey].totalGasUsed += parseInt(tx.gasUsed || 0);
    weeklyStats[weekKey].totalValue += parseFloat(tx.value) / 1e18; // Convert from wei to xDAI
  });

  // Sort weeks chronologically
  const sortedWeeks = Object.keys(weeklyStats).sort();

  console.log('=== WEEKLY BATCH TRANSACTION STATISTICS (Last 6 Months) ===\n');
  console.log('Week        | Count | Total Gas Used | Total Value (xDAI)');
  console.log('------------|-------|----------------|-------------------');

  let totalCount = 0;
  let totalGas = 0;
  let totalValue = 0;

  sortedWeeks.forEach(week => {
    const stats = weeklyStats[week];
    totalCount += stats.count;
    totalGas += stats.totalGasUsed;
    totalValue += stats.totalValue;

    console.log(
      `${week.padEnd(11)} | ${String(stats.count).padStart(5)} | ${String(stats.totalGasUsed).padStart(14)} | ${stats.totalValue.toFixed(4).padStart(17)}`
    );
  });

  console.log('------------|-------|----------------|-------------------');
  console.log(
    `${'TOTAL'.padEnd(11)} | ${String(totalCount).padStart(5)} | ${String(totalGas).padStart(14)} | ${totalValue.toFixed(4).padStart(17)}`
  );

  console.log('\n=== SUMMARY ===');
  console.log(`Total weeks with activity: ${sortedWeeks.length}`);
  console.log(`Average transactions per week: ${(totalCount / sortedWeeks.length).toFixed(2)}`);
  console.log(`Average gas per transaction: ${(totalGas / totalCount).toFixed(0)}`);

  // Find most active week
  if (sortedWeeks.length > 0) {
    const mostActiveWeek = sortedWeeks.reduce((max, week) =>
      weeklyStats[week].count > weeklyStats[max].count ? week : max
    , sortedWeeks[0]);

    console.log(`\nMost active week: ${mostActiveWeek} (${weeklyStats[mostActiveWeek].count} transactions)`);

    // Sample transaction hashes from most recent week
    const latestWeek = sortedWeeks[sortedWeeks.length - 1];
    console.log(`\nSample transactions from ${latestWeek}:`);
    weeklyStats[latestWeek].transactions.slice(0, 3).forEach(hash => {
      console.log(`  https://gnosisscan.io/tx/${hash}`);
    });
  }
}

// Run the analysis
analyzeTransactions().catch(console.error);
