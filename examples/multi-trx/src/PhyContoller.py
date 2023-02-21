import requests
import json

class PhyController:

    def __init__(self, url):
        self.url = url
        request = requests.get(url)
        request_json = request.json()
        self.blocks = request_json["blocks"]
        self.current_phy = 0

        # get id of relevant blocks
        source_selector_id = -1 
        sink_selector_id = -1
        message_selector_id = -1
        soapy_source_id = -1
        soapy_sink_id = -1

        for block in self.blocks:
            if block["instance_name"] == "Selector<2, 1>_0":
                source_selector_id = block["id"]
            if block["instance_name"] == "Selector<1, 2>_0":
                sink_selector_id = block["id"]
            if block["instance_name"] == "MessageSelector_0":
                message_selector_id = block["id"]

        if (source_selector_id == -1) or (sink_selector_id == -1) or (message_selector_id == -1):
            if source_selector_id == -1:
                print("Cannot find source selector!")
            if sink_selector_id == -1:
                print("Cannot find sink selector!")
            if message_selector_id == -1:
                print("Cannot find message selector")


        self.source_selector_url = "{0}block/{1}/call/0/".format(url, source_selector_id)
        self.sink_selector_url = "{0}block/{1}/call/1/".format(url, sink_selector_id)
        self.message_selector_url = "{0}block/{1}/call/1/".format(url, message_selector_id)

    #select the PHY protocol (WLAN = 0, Bluetooth =1)
    def select_phy(self, phy):
        requests.post(self.source_selector_url, json = {"U32" : phy})
        requests.post(self.sink_selector_url, json = {"U32" : phy})
        requests.post(self.message_selector_url, json = {"U32" : phy})
        self.current_phy = phy

    #switches to the other phy l
    def switch_phy(self):
        if self.current_phy == 0:
            self.current_phy = 1
        else:
            self.current_phy = 0
        requests.post(self.source_selector_url, json = {"U32" : self.current_phy})
        requests.post(self.sink_selector_url, json = {"U32" : self.current_phy})
        requests.post(self.message_selector_url, json = {"U32" : self.current_phy})
        



    
