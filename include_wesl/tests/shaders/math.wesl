public fn offset(x: u32) -> u32 {
    @if(gamma)
    return x + 1000u;
    @else
    return x + 2000u;
}
