const { default: data  } = await import("./data.json", {
    assert: {
        type: "json"
    }
});
