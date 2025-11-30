const https = require('https');

const CONTRACT_ADDRESS = '0x5ebfbefb1e88391efb022d5d33302f50a46bf4f3';

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
  const date = new Date(timestamp);
  const firstDayOfYear = new Date(date.getFullYear(), 0, 1);
  const pastDaysOfYear = (date - firstDayOfYear) / 86400000;
  const weekNumber = Math.ceil((pastDaysOfYear + firstDayOfYear.getDay() + 1) / 7);
  return `${date.getFullYear()}-W${String(weekNumber).padStart(2, '0')}`;
}

// Get the Monday (start) of a week from week key (e.g., "2025-W48")
function getWeekStartDate(weekKey) {
  const [year, weekStr] = weekKey.split('-W');
  const weekNumber = parseInt(weekStr);

  // January 1st of the year
  const firstDayOfYear = new Date(parseInt(year), 0, 1);

  // Calculate the first Monday of the year
  const firstMonday = new Date(firstDayOfYear);
  const dayOfWeek = firstDayOfYear.getDay();
  const daysUntilMonday = dayOfWeek === 0 ? 1 : (8 - dayOfWeek);
  firstMonday.setDate(firstDayOfYear.getDate() + daysUntilMonday);

  // Add weeks to get to the target week
  const targetDate = new Date(firstMonday);
  targetDate.setDate(firstMonday.getDate() + (weekNumber - 1) * 7);

  return targetDate.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' });
}

// Get date 1 year ago
function getOneYearAgo() {
  const date = new Date();
  date.setMonth(date.getMonth() - 12);
  return date.toISOString();
}

function formatDate(dateStr) {
  const date = new Date(dateStr);
  return date.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' });
}

async function fetchTransactions() {
  console.log('Fetching transactions for address:', CONTRACT_ADDRESS);
  console.log('Using Blockscout API...\n');

  const allTransactions = [];
  let nextPageUrl = `https://gnosis.blockscout.com/api/v2/addresses/${CONTRACT_ADDRESS}/transactions`;

  try {
    let page = 1;
    while (nextPageUrl && page <= 50) { // Limit to 50 pages to avoid infinite loops
      console.log(`Fetching page ${page}...`);

      const response = await apiRequest(nextPageUrl);

      if (!response || !response.items || response.items.length === 0) {
        break;
      }

      allTransactions.push(...response.items);
      console.log(`  Got ${response.items.length} transactions`);

      // Check for next page
      nextPageUrl = response.next_page_params ?
        `https://gnosis.blockscout.com/api/v2/addresses/${CONTRACT_ADDRESS}/transactions?${new URLSearchParams(response.next_page_params).toString()}` :
        null;

      page++;

      // Add a small delay to avoid rate limiting
      await new Promise(resolve => setTimeout(resolve, 300));
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
  const oneYearAgo = new Date(getOneYearAgo());

  // Filter transactions from last year
  const recentTxs = transactions.filter(tx => {
    const txDate = new Date(tx.timestamp);
    return txDate >= oneYearAgo;
  });

  console.log(`\nTransactions in last year: ${recentTxs.length}`);

  // Debug: Show the most recent transaction date
  if (recentTxs.length > 0) {
    const mostRecent = recentTxs.reduce((latest, tx) => {
      const latestDate = new Date(latest.timestamp);
      const txDate = new Date(tx.timestamp);
      return txDate > latestDate ? tx : latest;
    });
    console.log(`Most recent transaction: ${new Date(mostRecent.timestamp).toLocaleString()} - ${mostRecent.method}`);
  }

  // Filter for "create batch" or batch-related transactions
  const batchTxs = recentTxs.filter(tx => {
    const method = (tx.method || '').toLowerCase();
    const type = (tx.tx_types || []).join(' ').toLowerCase();

    return method.includes('batch') ||
           method.includes('multi') ||
           type.includes('batch') ||
           (tx.to && tx.to.name && tx.to.name.toLowerCase().includes('batch'));
  });

  console.log(`Batch-related transactions found: ${batchTxs.length}\n`);

  // Show all unique methods found
  const uniqueMethods = [...new Set(recentTxs.map(tx => tx.method).filter(Boolean))];
  console.log('All methods found in recent transactions:');
  uniqueMethods.forEach(method => console.log(`  - ${method}`));
  console.log('');

  // Group by week
  const weeklyStats = {};

  batchTxs.forEach(tx => {
    const weekKey = getWeekKey(tx.timestamp);
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
    weeklyStats[weekKey].totalGasUsed += parseInt(tx.gas_used || 0);
    weeklyStats[weekKey].totalValue += parseFloat(tx.value || 0) / 1e18;
  });

  // Sort weeks chronologically
  const sortedWeeks = Object.keys(weeklyStats).sort();

  console.log('=== WEEKLY BATCH TRANSACTION STATISTICS (Last Year) ===\n');
  console.log('Week        | Week Starting  | Count | Total Gas Used | Total Value (xDAI)');
  console.log('------------|----------------|-------|----------------|-------------------');

  let totalCount = 0;
  let totalGas = 0;
  let totalValue = 0;

  sortedWeeks.forEach(week => {
    const stats = weeklyStats[week];
    const weekStart = getWeekStartDate(week);
    totalCount += stats.count;
    totalGas += stats.totalGasUsed;
    totalValue += stats.totalValue;

    console.log(
      `${week.padEnd(11)} | ${weekStart.padEnd(14)} | ${String(stats.count).padStart(5)} | ${String(stats.totalGasUsed).padStart(14)} | ${stats.totalValue.toFixed(4).padStart(17)}`
    );
  });

  console.log('------------|----------------|-------|----------------|-------------------');
  console.log(
    `${'TOTAL'.padEnd(11)} | ${''.padEnd(14)} | ${String(totalCount).padStart(5)} | ${String(totalGas).padStart(14)} | ${totalValue.toFixed(4).padStart(17)}`
  );

  if (sortedWeeks.length > 0) {
    console.log('\n=== SUMMARY ===');
    console.log(`Total weeks with activity: ${sortedWeeks.length}`);
    console.log(`Average transactions per week: ${(totalCount / sortedWeeks.length).toFixed(2)}`);
    console.log(`Average gas per transaction: ${(totalGas / totalCount).toFixed(0)}`);

    // Find most active week
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
