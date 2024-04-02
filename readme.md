# Toy payments engine

## Assumptions

#### 1)
I have assumed only deposits can be disputed. The behaviour of chargebacks did not seem to make sense otherwise. e.g consider the example: 

|type|client|tx|amount|
|----|-----|---|------|
|deposit|1|1|100|
|withdrawal|1|2|50|
|dispute|1|2||
|chargeback|1|2||


According to the specification disputes should result in available funds _decreasing_ and held funds _increasing_ by the transaction amount in question. If applied to withdrawals as in the above example, taken naively this would reduce the available funds by a further 50. And as the result of the chargeback reduce the total funds by 50. The client would be charged double the amount disputed, and result in this account:

|client|available|held|total|locked|
|-----|----------|----|-----|------|
|1|0|0|0|true|

#### 2)

Transactions can only have one dispute made against them at a time

#### 3)

Transactions can be retrospectively accepted if they were rejected after a dispute. 


## Comments
im trait used for immutable datatypes








 
