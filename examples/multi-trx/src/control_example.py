import PhyController
import argparse
import tkinter

parser = argparse.ArgumentParser(
    prog = 'control',
    description = 'Sends messages to the FutureSDR flowgraph API to switch PHY layers of the UAV transmission')
parser.add_argument("-u", "--url", default = "http://127.0.0.1:1337/api/fg/0/") #url of flowgraph api

args = parser.parse_args()
flowgraph_url = args.url
control = PhyController.PhyController(flowgraph_url)

#GUI
gui = tkinter.Tk()
gui.title("PHY layer switch")
gui.geometry("200x100")
button_wlan = tkinter.Button(gui, command=lambda : control.select_phy(0))
button_wlan['text'] = "WLAN"
button_wlan.pack()
button_zigbee = tkinter.Button(gui, command=lambda : control.select_phy(1))
button_zigbee['text'] = "Zigbee"
button_zigbee.pack()

gui.mainloop()