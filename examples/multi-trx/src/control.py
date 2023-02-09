import requests
import json
import argparse
import tkinter

parser = argparse.ArgumentParser(
    prog = 'control',
    description = 'Sends messages to the FutureSDR flowgraph API to switch PHY layers of the UAV transmission')
parser.add_argument("-u", "--url", default = "http://127.0.0.1:1337/api/fg/0/") #url of flowgraph api

args = parser.parse_args()
flowgraph_url = args.url

request = requests.get(flowgraph_url)
request_json = request.json()
blocks = request_json["blocks"]

# get id of relevant blocks
source_selector_id = -1 
sink_selector_id = -1
message_selector_id = -1

for block in blocks:
    if block["instance_name"] == "Selector<2, 1>_0":
        source_selector_id = block["id"]
    if block["instance_name"] == "Selector<1, 2>_0":
        sink_selector_id = block["id"]
    if block["instance_name"] == "MessageSelector_0":
        message_selector_id = block["id"]

# exit the script if one of the relevant blocks cannot be found
if (source_selector_id == -1) or (sink_selector_id == -1) or (message_selector_id == -1):
    if source_selector_id == -1:
        print("Cannot find source selector!")
    if sink_selector_id == -1:
        print("Cannot find sink selector!")
    if message_selector_id == -1:
        print("Cannot find message selector")
    exit()

source_selector_url = "{0}block/{1}/call/0/".format(flowgraph_url, source_selector_id)
sink_selector_url = "{0}block/{1}/call/1/".format(flowgraph_url, sink_selector_id)
message_selector_url = "{0}block/{1}/call/1/".format(flowgraph_url, message_selector_id)

# set the protocol to use for the transmission
def set_protocol(protocol_number):
    requests.post(source_selector_url, json = {"U32" : protocol_number})
    requests.post(sink_selector_url, json = {"U32" : protocol_number})
    requests.post(message_selector_url, json = {"U32" : protocol_number})

# GUI
gui = tkinter.Tk()
gui.title("PHY layer switch")
gui.geometry("100x100")
button_wlan = tkinter.Button(gui, command=lambda : set_protocol(0))
button_wlan['text'] = "WLAN"
button_wlan.pack()
button_bluetooth = tkinter.Button(gui, command=lambda : set_protocol(1))
button_bluetooth['text'] = "Bluetooth"
button_bluetooth.pack()

gui.mainloop()