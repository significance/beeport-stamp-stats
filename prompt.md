create a batch cache that reflects the batch normalised balance at the point of caching
also store the block that the data was cached at
when doing the analytics, fetch the current block and recalculate the expected expiry based on the price specified in the analytics cli command flag (or the default, which should be the last retrieved price during the last caching fetch)
do not recache the data when running the analytics command unless the --refresh flag is specified


