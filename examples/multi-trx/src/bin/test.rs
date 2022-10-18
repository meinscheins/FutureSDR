use futuresdr::anyhow::Result;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::blocks::Selector;
use futuresdr::blocks::SelectorDropPolicy as DropPolicy;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;


fn main() -> Result <()>{

    let mut fg = Flowgraph::new();

    let selector = Selector::<Complex32, 1, 2>::new(DropPolicy::SameRate);
    let selector = fg.add_block(selector);
    let snk1= fg.add_block(NullSink::<Complex32>::new());
    let snk2= fg.add_block(NullSink::<Complex32>::new());
    let src= fg.add_block(NullSource::<Complex32>::new());

    fg.connect_stream(src, "out", selector, "in0")?;
    fg.connect_stream(selector, "out0", snk1, "in")?;
    fg.connect_stream(selector, "out1", snk2, "in")?;
    
    let rt = Runtime::new();
    rt.run(fg)?;
    Ok(())
}