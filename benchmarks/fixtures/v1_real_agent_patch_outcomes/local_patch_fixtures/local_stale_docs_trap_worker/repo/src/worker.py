def launch_worker(options):
    retries = options.get("retries", 3)
    return {"status": "running", "retries": retries}
