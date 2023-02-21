import requests
import json

class PhyController:

    def __init__(self, url):
        self.url = url
        request = requests.get(url)
        request_json = request.json()
        self.blocks = request_json["blocks"]
        self.current_phy = 0
        self.rx_freq = [5170e6, 2480e6]
        self.tx_freq = [5170e6, 2480e6]
        self.rx_gain = [60, 50]
        self.tx_gain = [60, 50]
        self.sample_rate = [20e6, 40e6]

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
            if block["instance_name"] == "SoapySink_0":
                soapy_sink_id = block["id"]
            if block["instance_name"] == "SoapySource_0":
                soapy_source_id = block["id"]

        if (source_selector_id == -1) or (sink_selector_id == -1) or (soapy_source_id == -1) or (soapy_sink_id == -1) or (message_selector_id == -1): 
            if source_selector_id == -1:
                print("Cannot find source selector!")
            if sink_selector_id == -1:
                print("Cannot find sink selector!")
            if soapy_source_id == -1:
                print("Cannot find soapy source")
            if soapy_sink_id == -1:
                print("Cannot find soapy sink")
            if message_selector_id == -1:
                print("Cannot find message selector")


        self.source_selector_url = "{0}block/{1}/call/0/".format(url, source_selector_id)
        self.sink_selector_url = "{0}block/{1}/call/1/".format(url, sink_selector_id)
        self.message_selector_url = "{0}block/{1}/call/1/".format(url, message_selector_id)
        self.soapy_source_freq_url = "{0}block/{1}/call/0/".format(url, soapy_source_id)
        self.soapy_source_gain_url = "{0}block/{1}/call/1/".format(url, soapy_source_id)
        self.soapy_source_sample_rate_url = "{0}block/{1}/call/2/".format(url, soapy_source_id)
        self.soapy_sink_freq_url = "{0}block/{1}/call/0/".format(url, soapy_sink_id)
        self.soapy_sink_gain_url = "{0}block/{1}/call/1/".format(url, soapy_sink_id)
        self.soapy_sink_sample_rate_url = "{0}block/{1}/call/2/".format(url, soapy_sink_id)

    #sets rx frequency of the corresponing PHY layer
    def set_rx_frequency(self, phy, frequency):
        self.rx_freq[phy] = frequency
    
    #sets tx frequency of the corresponing PHY layer
    def set_tx_frequency(self, phy, frequency):
        self.tx_freq[phy] = frequency

    #set rx gain of the corresponding PHY layer
    def set_rx_gain(self, phy, frequency):
        self.rx_gain[phy] = frequency

    #set tx gain of the corresponding PHY layer
    def set_tx_gain(self, phy, gain):
        self.tx_gain[phy] = gain

    #sets sample rate of the corresponding PHY layer 
    def set_sample_rate(self, phy, sample_rate):
        self.sample_rate[phy] = sample_rate

    #select the PHY protocol (WLAN = 0, Bluetooth =1)
    def select_phy(self, phy):
        requests.post(self.source_selector_url, json = {"U32" : phy})
        requests.post(self.sink_selector_url, json = {"U32" : phy})
        requests.post(self.message_selector_url, json = {"U32" : phy})
        requests.post(self.soapy_source_freq_url, json = {"F64" : int(self.rx_freq[phy])})
        requests.post(self.soapy_source_gain_url, json = {"F64" : int(self.rx_gain[phy])})
        requests.post(self.soapy_source_sample_rate_url, json = {"F64" : int(self.sample_rate[phy])})
        requests.post(self.soapy_sink_freq_url, json = {"F64" : int(self.tx_freq[phy])})
        requests.post(self.soapy_sink_gain_url, json = {"F64" : int(self.tx_gain[phy])})
        requests.post(self.soapy_sink_sample_rate_url, json = {"F64" : int(self.sample_rate[phy])})

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
        requests.post(self.soapy_source_freq_url, json = {"F64" : self.rx_freq[self.current_phy]})
        requests.post(self.soapy_source_gain_url, json = {"F64" : self.rx_gain[self.current_phy]})
        requests.post(self.soapy_source_sample_rate_url, json = {"F64" : self.sample_rate[self.current_phy]})
        requests.post(self.soapy_sink_freq_url, json = {"F64" : self.tx_freq[self.current_phy]})
        requests.post(self.soapy_sink_gain_url, json = {"F64" : self.tx_gain[self.current_phy]})
        requests.post(self.soapy_sink_sample_rate_url, json = {"F64" : self.sample_rate[self.current_phy]})
        



    
